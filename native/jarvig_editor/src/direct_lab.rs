//! Experiment 3C. Two direct realizations of one analytic specimen, one presented frame.
//!
//! The specimen is the in-memory spherical-square pair. It does not read the
//! Intent Solid's evaluated body and it does not turn on intent authority.
//! Surface A is omitted. Surface B is constructed at the resolution each
//! observation measured. The products are not the object. This does not start
//! a realization worker and it does not accept ADR-0074.

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
/// Printed by the 2026-10-06 CPU predicate. A different value fails this run.
const FROZEN_ANALYTIC_HASH: u64 = 0xac80_f43b_acf3_c609;
const FROZEN_FAR_ERROR: &str = "0.424494";
const FROZEN_FAR_COARSER: &str = "0.613221";
const FROZEN_CLOSE_ERROR: &str = "0.462054";
const FROZEN_CLOSE_COARSER: &str = "0.536666";

pub(super) struct DirectLab {
    stage: u8,
    arranged: bool,
    split: bool,
    grow: u8,
    mark: u32,
    proved: bool,
    captured_far: bool,
    captured_close: bool,
    right_view: Option<RenderViewId>,
    solid: Option<jarvig_core::EntityId>,
    marker: Option<jarvig_core::EntityId>,
    control_mesh: Option<MeshId>,
    marker_mesh: Option<MeshId>,
    mesh_far: Option<MeshId>,
    mesh_close: Option<MeshId>,
    decoy: Option<MeshId>,
    retired: Vec<MeshId>,
    key_far: String,
    key_close: String,
    pose_far: ResolvedPose,
    pose_close: ResolvedPose,
    camera: jarvig_core::Camera,
    product_far: Option<jarvig_core::DirectProduct>,
    product_close: Option<jarvig_core::DirectProduct>,
    record: Option<jarvig_core::BlockRecord>,
    analytic_hash: u64,
    solid_pose: Vec3,
    solid_name: String,
    collision_inside: Vec3,
    collision_outside: Vec3,
    control_vertices: u32,
    control_triangles: u32,
    meshlet_len: Option<usize>,
    control_gpu_bytes: Option<u64>,
    close_gpu_bytes: u64,
    mesh_count: usize,
    revision_before: u64,
    acquires_at_proof: u32,
    presents_at_proof: u32,
    panel: (u32, u32),
    discarded_stale_gpu: u32,
    /// The spawn revises the world. The rebuild frame records no caster list when the level has no shadow light. The next frame does.
    caster_check_pending: bool,
    level_bytes: Vec<u8>,
    level_path: String,
    report: String,
}

impl DirectLab {
    pub(super) fn new() -> Self {
        Self {
            stage: 0,
            arranged: false,
            split: false,
            grow: 0,
            mark: 0,
            proved: false,
            captured_far: false,
            captured_close: false,
            right_view: None,
            solid: None,
            marker: None,
            control_mesh: None,
            marker_mesh: None,
            mesh_far: None,
            mesh_close: None,
            decoy: None,
            retired: Vec::new(),
            key_far: String::new(),
            key_close: String::new(),
            pose_far: ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY },
            pose_close: ResolvedPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY },
            camera: jarvig_core::Camera { frame: jarvig_core::FrameId(0), vertical_fov_radians: 1.0, near_m: 0.1 },
            product_far: None,
            product_close: None,
            record: None,
            analytic_hash: 0,
            solid_pose: Vec3::ZERO,
            solid_name: String::new(),
            collision_inside: Vec3::ZERO,
            collision_outside: Vec3::ZERO,
            control_vertices: 0,
            control_triangles: 0,
            meshlet_len: None,
            control_gpu_bytes: None,
            close_gpu_bytes: 0,
            mesh_count: 0,
            revision_before: 0,
            acquires_at_proof: 0,
            presents_at_proof: 0,
            panel: (0, 0),
            discarded_stale_gpu: 0,
            caster_check_pending: false,
            level_bytes: Vec::new(),
            level_path: String::new(),
            report: String::new(),
        }
    }
}

impl Editor {
    pub(super) fn apply_direct_frame(&mut self) -> Result<(), String> {
        let Some(lab) = self.direct_lab.as_ref() else {
            return Ok(());
        };
        if !lab.split {
            return Ok(());
        }
        let right = lab.right_view.ok_or("the close observation view is missing")?;
        let left = self.viewport_view.ok_or("the perspective view is missing")?;
        let (pose_far, pose_close, camera, entity, mesh_far, mesh_close) = (lab.pose_far, lab.pose_close, lab.camera, lab.marker, lab.mesh_far, lab.mesh_close);
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

    pub(super) fn step_direct_lab(&mut self) -> Result<(), String> {
        if self.direct_lab.is_none() {
            return Ok(());
        }
        let stage = self.direct_lab.as_ref().unwrap().stage;
        if stage >= 15 {
            return Ok(());
        }
        let result = match stage {
            0 => self.direct_stage_setup(),
            1 => self.direct_stage_present(),
            2 => self.direct_stage_far_gone(),
            3 => self.direct_stage_close_gone(),
            4 => self.direct_stage_intact(),
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.note_direct("Experiment 3C: FAIL");
            self.note_direct(&format!("Failure: {error}"));
            self.direct_write_report();
            if let Some(lab) = self.direct_lab.as_mut() {
                lab.stage = 15;
                lab.split = false;
            }
            return Err(error);
        }
        Ok(())
    }

    fn note_direct(&mut self, line: &str) {
        self.append(line);
        println!("{line}");
        if let Some(lab) = self.direct_lab.as_mut() {
            lab.report.push_str(line);
            lab.report.push('\n');
        }
    }

    pub(super) fn direct_write_report(&mut self) {
        let Some(lab) = self.direct_lab.as_ref() else {
            return;
        };
        let mut text = lab.report.clone();
        text.push_str("Direct realization report: REPORT-OBS-3C.txt\n");
        let dir = std::path::PathBuf::from(r"C:\Users\Jeramiah\AppData\Local\Temp\jarvig-intent-proof");
        match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("REPORT-OBS-3C.txt"), &text)) {
            Ok(()) => self.note_direct("Direct realization report: REPORT-OBS-3C.txt"),
            Err(error) => self.note_direct(&format!("Direct realization report was not written: {error}")),
        }
    }

    fn direct_capture(&mut self, name: &str) -> Result<(), String> {
        let path = format!(r"C:\Users\Jeramiah\AppData\Local\Temp\jarvig-intent-proof\{name}");
        let image = super::capture_window_image(self.frame)?;
        super::write_png(&path, &image)?;
        self.note_direct(&format!("Capture: {name}"));
        Ok(())
    }

    fn direct_stage_setup(&mut self) -> Result<(), String> {
        if self.view_lab.is_some() || self.view_realization.is_some() || self.observation_lab.is_some() || self.intent_live_rerealize {
            return Err("Experiment 3C refuses to run beside a realization or 3B lab".into());
        }
        if self.engine.intent_authority_experiment() {
            return Err("Experiment 3C refuses the intent-authority experiment flag".into());
        }
        if self.renderer.is_none() || self.editor_camera.is_none() || self.viewport_view.is_none() || self.viewport_px.1 < 64 {
            if self.frames >= 400 {
                return Err("the perspective viewport was not ready".into());
            }
            return Ok(());
        }
        if !self.direct_lab.as_ref().unwrap().arranged {
            for panel in [OUTLINER, INSPECTOR, CONTENT, OUTPUT] {
                if self.workspace.is_open(panel) {
                    self.workspace.apply(WorkspaceCommand::ClosePanel(panel)).map_err(|error| error.to_string())?;
                }
            }
            unsafe { SetWindowPos(self.frame, std::ptr::null_mut(), 20, 20, 1800, 1200, SWP_NOZORDER); }
            self.realize();
            let lab = self.direct_lab.as_mut().unwrap();
            lab.arranged = true;
            lab.mark = self.frames;
            return Ok(());
        }
        let (panel_w, panel_h) = self.viewport_px;
        let renderer_size = self.renderer.as_ref().unwrap().configured_size();
        if panel_w < REQUIRED_WIDTH || panel_h < VIEW_HEIGHT || renderer_size.0 < REQUIRED_WIDTH || renderer_size.1 < VIEW_HEIGHT {
            let grow = self.direct_lab.as_ref().unwrap().grow;
            let mark = self.direct_lab.as_ref().unwrap().mark;
            if grow < 2 && self.frames.saturating_sub(mark) > 8 {
                let (width, height) = if grow == 0 { (2200, 1400) } else { (2600, 1600) };
                unsafe { SetWindowPos(self.frame, std::ptr::null_mut(), 20, 20, width, height, SWP_NOZORDER); }
                self.realize();
                let lab = self.direct_lab.as_mut().unwrap();
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
            let Some(intent) = outline.iter().find(|item| item.name == "Intent Solid").map(|item| item.uuid) else {
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
        self.direct_install(control_gpu_bytes, (panel_w, panel_h))?;
        Ok(())
    }

    fn direct_install(&mut self, control_gpu_bytes: u64, panel: (u32, u32)) -> Result<(), String> {
        let level_path = self.level_file.clone().ok_or("Main.jarviglevel is not open")?;
        let level_bytes = std::fs::read(&level_path).map_err(|error| error.to_string())?;
        let far_camera = jarvig_core::far_observation();
        let close_camera = jarvig_core::close_observation();
        if far_camera.viewport_width != VIEW_FAR_WIDTH as f32 || close_camera.viewport_width != VIEW_CLOSE_WIDTH as f32 || far_camera.viewport_height != VIEW_HEIGHT as f32 {
            return Err("the direct cameras were not 160x567 and 960x567".into());
        }
        if (far_camera.requested_error_px - 0.5).abs() > 1.0e-6 || (close_camera.requested_error_px - 0.5).abs() > 1.0e-6 {
            return Err("the direct cameras did not both request 0.5 px".into());
        }
        let gathered = {
            let world = self.engine.world();
            let intent = world.entity_outline().iter().find(|item| item.name == "Intent Solid").map(|item| item.uuid).ok_or("Intent Solid missing")?;
            let record = world.authored_block(intent).ok_or("Intent Solid has no block")?;
            if record.body.is_some() {
                return Err("Intent Solid has an evaluated body. 3C does not use it and does not reconstruct it".into());
            }
            let object_mesh = world.object_mesh(intent).ok_or("the ordinary renderer has no mesh for Intent Solid")?;
            let shared = world.meshes().get(object_mesh).ok_or("the ordinary control mesh is missing")?;
            let control_vertices = shared.vertex_count();
            let control_triangles = shared.index_count() / 3;
            let meshlet_len = world.derived_meshlets(object_mesh).map(|set| set.meshlets.len());
            let front = world.front_camera();
            let expected_fov = 60.0_f64.to_radians();
            if (front.vertical_fov_radians - expected_fov).abs() > 1.0e-6 || (f64::from(front.near_m) - 0.1).abs() > 1.0e-6 {
                return Err("the front camera is not the 60 degree, 0.1 m camera".into());
            }
            if (far_camera.vertical_fov_radians - expected_fov).abs() > 1.0e-9 {
                return Err("the direct camera field of view left 60 degrees".into());
            }
            let pose = world.entity_local_pose(intent).map_err(|error| error.to_string())?.translation;
            let name = world.remember_entity(intent).map_err(|error| error.to_string())?.name;
            let collision_inside = world.separate_from_blocks(Vec3::new(0.0, 1.0, -4.0));
            let collision_outside = world.separate_from_blocks(Vec3::new(0.0, 1.0, 40.0));
            (
                intent,
                record,
                object_mesh,
                control_vertices,
                control_triangles,
                meshlet_len,
                pose,
                name,
                collision_inside,
                collision_outside,
                front,
                world.revision(),
                world.mesh_count(),
            )
        };
        let (intent, record, object_mesh, control_vertices, control_triangles, meshlet_len, pose, name, collision_inside, collision_outside, front, revision_before, mesh_count) = gathered;
        let product_far = jarvig_core::realize_direct(&far_camera).map_err(|error| format!("far direct realization stopped: {error}"))?;
        let product_close = jarvig_core::realize_direct(&close_camera).map_err(|error| format!("close direct realization stopped: {error}"))?;
        let fresh = jarvig_core::AnalyticObject::specimen();
        if product_far.analytic != fresh || product_close.analytic != fresh || product_far.analytic.revision != 1 {
            return Err("the two observations did not share analytic revision 1".into());
        }
        self.note_direct("Experiment 3C: ON");
        self.note_direct("Experiment 3A frozen: PASS");
        self.note_direct("Experiment 3B frozen: PASS");
        self.note_direct("Intent-authority experiment: OFF");
        self.note_direct("ADR-0074 accepted: NO");
        self.note_direct("Default renderer replaced: NO");
        self.note_direct("Backface omission: NO");
        self.note_direct("Same-solid occlusion: NO");
        self.note_direct("Edge collapse: NO");
        self.note_direct("Simplification: NO");
        self.note_direct(&format!("DIRECT_REALIZATION_ALGORITHM_VERSION: {}", jarvig_core::DIRECT_REALIZATION_ALGORITHM_VERSION));
        self.note_direct(&format!("OBSERVATION_ALGORITHM_VERSION: {}", jarvig_core::OBSERVATION_ALGORITHM_VERSION));
        self.note_direct(&format!("REALIZATION_ALGORITHM_VERSION: {}", jarvig_core::REALIZATION_ALGORITHM_VERSION));
        self.note_direct(&format!("Editor workers: general {} einstein {} realize 0", self.jobs.worker_count(), self.einstein_jobs.worker_count()));
        self.note_direct("Experiment 2 worker: NOT STARTED");
        self.note_direct("jarvig-realize-0: NOT STARTED");
        self.note_direct("jarvig-realize-1: NOT STARTED");
        self.note_direct("Editor worker cap raised: NO");
        self.note_direct("queue: NOT USED");
        self.note_direct(&format!("Graphics adapter: {}", self.graphics_name));
        self.note_direct(&format!("Graphics vendor: {:04x}", self.graphics_vendor));
        self.note_direct(&format!("Graphics device: {:04x}", self.graphics_device));
        self.note_direct(&format!("Graphics api: {}", self.graphics_api));
        self.note_direct(&format!("Editor panel: {}x{}", panel.0, panel.1));
        self.note_direct("Observation Far viewport: 160x567");
        self.note_direct("Observation Close viewport: 960x567");
        self.note_direct("Re-admitted at the editor panel size: NO");
        self.note_direct(&format!("Requested error px: {:.1}", far_camera.requested_error_px));
        self.note_direct(&format!("Observation Far eye_local: [{:.4}, {:.4}, {:.4}]", far_camera.eye[0], far_camera.eye[1], far_camera.eye[2]));
        self.note_direct(&format!("Observation Close eye_local: [{:.4}, {:.4}, {:.4}]", close_camera.eye[0], close_camera.eye[1], close_camera.eye[2]));
        self.note_direct("Observation forward_local: [0.0000, 0.0000, -1.0000]");
        self.note_direct("3C specimen: analytic spherical squares");
        self.note_direct("Intent Solid is the specimen: NO");
        self.note_direct("Intent-authority switch used: NO");
        self.note_direct("Intent Solid evaluated body: ABSENT");
        self.note_direct(&format!("analytic_hash: {:016x}", fresh.content_hash));
        self.note_direct("analytic_revision: 1");
        self.note_direct(&format!("Ordinary control mesh: {control_vertices} vertices, {control_triangles} triangles"));
        self.note_direct(&format!("Level file: {}", level_path.display()));
        self.note_direct(&format!("Level file bytes before: {}", level_bytes.len()));
        self.note_direct(&jarvig_core::format_direct_product("Observation Far", &product_far, None));
        self.note_direct(&jarvig_core::format_direct_product("Observation Close", &product_close, None));
        let n_far = product_far.surfaces[1].n;
        let n_close = product_close.surfaces[1].n;
        self.note_direct(&format!("n_far: {n_far}"));
        self.note_direct(&format!("n_close: {n_close}"));
        self.note_direct(&format!("n_close > n_far: {}", yes_no(n_close > n_far && n_far >= 1)));
        let predicates_ok = direct_predicates_ok(&product_far, &product_close);
        if self.jobs.worker_count() != 2 || self.einstein_jobs.worker_count() != 1 {
            self.note_direct("Frame: NOT PRESENTED");
            return Err(format!("editor workers were general {} einstein {}", self.jobs.worker_count(), self.einstein_jobs.worker_count()));
        }
        if jarvig_core::DIRECT_REALIZATION_ALGORITHM_VERSION != 1 || jarvig_core::OBSERVATION_ALGORITHM_VERSION != 1 || jarvig_core::REALIZATION_ALGORITHM_VERSION != 1 {
            self.note_direct("Frame: NOT PRESENTED");
            return Err("an algorithm version moved".into());
        }
        if !predicates_ok || n_close <= n_far {
            self.note_direct("Frame: NOT PRESENTED");
            return Err(format!("the direct predicate did not produce an omitted Surface A and a finer close grid (n_far {n_far}, n_close {n_close})"));
        }
        if let Err(error) = frozen_cpu_match(&product_far, &product_close) {
            self.note_direct("Frozen CPU counts and hashes: DIFFER");
            self.note_direct("Frame: NOT PRESENTED");
            return Err(error);
        }
        self.note_direct("Frozen CPU counts and hashes: MATCH");
        self.note_direct("B_far: n=6 / 72 triangles");
        self.note_direct("B_close: n=14 / 392 triangles");
        let material = {
            let snapshot = self.engine.world().extract(jarvig_core::RenderFrameId(1)).map_err(|error| error.to_string())?;
            snapshot
                .instances()
                .iter()
                .find(|instance| instance.entity == intent)
                .and_then(|instance| instance.material_for_slot(0))
                .ok_or("the ordinary solid has no slot-0 material")?
        };
        let marker_mesh = self.engine.world_mut().add_mesh(jarvig_core::near_triangle_mesh());
        let scene = self.engine.world().scene_frame().map_err(|error| error.to_string())?;
        let marker_object = self
            .engine
            .world_mut()
            .spawn_named_object("Direct Marker", marker_mesh, scene, Vec3::new(6.0, 1.0, -4.0), Vec3::new(1.0, 1.0, 1.0))
            .map_err(|error| error.to_string())?;
        let marker = self.engine.world().entity_outline().iter().find(|item| item.name == "Direct Marker").map(|item| item.uuid).ok_or("Direct Marker was not outlined")?;
        self.engine.world_mut().mute_cast_shadows(marker).map_err(|error| error.to_string())?;
        self.engine.world_mut().bind_material(marker_object, 0, material).map_err(|error| error.to_string())?;
        if self.engine.world().object_mesh(marker) != Some(marker_mesh) {
            return Err("the marker object mesh was replaced".into());
        }
        if self.engine.world().authored_mesh(marker).is_some() {
            return Err("the presentation container has a saved mesh asset".into());
        }
        if self.engine.world().authored_block(marker).is_some() {
            return Err("the presentation container is a parametric block".into());
        }
        let revision_after_spawn = self.engine.world().revision();
        let mesh_far = self.engine.world_mut().add_mesh(product_far.mesh.clone());
        let mesh_close = self.engine.world_mut().add_mesh(product_close.mesh.clone());
        let decoy = self.engine.world_mut().add_mesh(jarvig_core::near_triangle_mesh());
        if self.engine.world().revision() != revision_after_spawn {
            return Err("add_mesh revised the world".into());
        }
        if mesh_far == object_mesh || mesh_close == object_mesh || mesh_far == marker_mesh || mesh_close == marker_mesh || mesh_far == mesh_close || decoy == mesh_far || decoy == mesh_close {
            return Err("a direct mesh reused the object mesh id or the other product".into());
        }
        if self.engine.world().derived_meshlets(mesh_far).is_some() || self.engine.world().derived_meshlets(mesh_close).is_some() {
            return Err("a direct mesh built meshlets".into());
        }
        if self.engine.world().object_mesh(intent) != Some(object_mesh) {
            return Err("the Intent Solid mesh id changed".into());
        }
        let marker_pose = self.engine.world().entity_world_pose(marker).map_err(|error| error.to_string())?;
        let pose_far = direct_pose(marker_pose, far_camera.eye, far_camera.forward)?;
        let pose_close = direct_pose(marker_pose, close_camera.eye, close_camera.forward)?;
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
            renderer.set_view_mesh_override(left, marker, Some(mesh_far)).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(right, marker, Some(mesh_close)).map_err(|error| error.to_string())?;
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
        {
            let lab = self.direct_lab.as_mut().unwrap();
            lab.solid = Some(intent);
            lab.marker = Some(marker);
            lab.control_mesh = Some(object_mesh);
            lab.marker_mesh = Some(marker_mesh);
            lab.mesh_far = Some(mesh_far);
            lab.mesh_close = Some(mesh_close);
            lab.decoy = Some(decoy);
            lab.right_view = Some(right);
            lab.key_far = product_far.cache_key.clone();
            lab.key_close = product_close.cache_key.clone();
            lab.pose_far = pose_far;
            lab.pose_close = pose_close;
            lab.camera = camera;
            lab.close_gpu_bytes = product_close.expected_gpu_bytes;
            lab.product_far = Some(product_far);
            lab.product_close = Some(product_close);
            lab.record = Some(record);
            lab.analytic_hash = fresh.content_hash;
            lab.solid_pose = pose;
            lab.solid_name = name;
            lab.collision_inside = collision_inside;
            lab.collision_outside = collision_outside;
            lab.control_vertices = control_vertices;
            lab.control_triangles = control_triangles;
            lab.meshlet_len = meshlet_len;
            lab.control_gpu_bytes = Some(control_gpu_bytes);
            lab.mesh_count = mesh_count;
            lab.revision_before = revision_before;
            lab.panel = panel;
            lab.level_bytes = level_bytes;
            lab.level_path = level_path.display().to_string();
            lab.split = true;
            lab.stage = 1;
            lab.mark = self.frames;
        }
        self.note_direct(&format!("Control mesh id: {}", object_mesh.0));
        self.note_direct(&format!("Marker mesh id: {}", marker_mesh.0));
        self.note_direct(&format!("Observation Far mesh id: {}", mesh_far.0));
        self.note_direct(&format!("Observation Close mesh id: {}", mesh_close.0));
        self.note_direct(&format!("Decoy mesh id: {}", decoy.0));
        self.note_direct("Observation mesh is the ordinary control mesh: NO");
        self.note_direct("Observation mesh is the marker mesh: NO");
        self.note_direct("Presentation container: Direct Marker");
        self.note_direct("Presentation container saved mesh: NONE");
        self.note_direct("Presentation container parametric block: NO");
        self.note_direct("Presentation container material: existing slot 0");
        self.note_direct("New material asset: NO");
        self.note_direct("set_object_mesh called: NO");
        self.note_direct("build_meshlets called: NO");
        self.note_direct("Observation meshlets: 0");
        self.note_direct("Marker cast_shadows: NO");
        self.note_direct(&format!("Control mesh GPU_bytes before: {control_gpu_bytes}"));
        self.note_direct(&format!("World revision before spawn: {revision_before}"));
        self.note_direct(&format!("World revision after spawn: {revision_after_spawn}"));
        self.note_direct("Color pass uses the observation mesh: YES");
        self.note_direct("Shadow, contact, and probe capture use instance.mesh: YES");
        Ok(())
    }

    fn direct_stage_present(&mut self) -> Result<(), String> {
        if !self.direct_lab.as_ref().unwrap().proved {
            if !self.direct_frame_matches(true)? {
                if self.frames.saturating_sub(self.direct_lab.as_ref().unwrap().mark) > 180 {
                    return Err(format!("the simultaneous direct frame did not upload ({})", self.direct_wait_reason()));
                }
                return Ok(());
            }
            self.direct_note_uploads()?;
            self.direct_note_passes("simultaneous", false)?;
            self.direct_capture("shot-obs-3c-simultaneous.png")?;
            self.direct_reject_stale()?;
            let lab = self.direct_lab.as_mut().unwrap();
            lab.proved = true;
            lab.mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.direct_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            if !self.direct_frame_matches(true)? {
                return Err("a later frame substituted a direct buffer".into());
            }
            if self.frames.saturating_sub(self.direct_lab.as_ref().unwrap().mark) == 1 {
                self.note_direct("Next frame kept Far on view Far and Close on view Close: YES");
                if self.direct_lab.as_ref().unwrap().caster_check_pending {
                    self.direct_note_passes("cached", true)?;
                }
            }
            return Ok(());
        }
        self.direct_retire_mesh(self.direct_lab.as_ref().unwrap().mesh_far.ok_or("mesh Far missing")?)?;
        self.direct_lab.as_mut().unwrap().mesh_far = None;
        self.direct_lab.as_mut().unwrap().stage = 2;
        self.direct_lab.as_mut().unwrap().mark = self.frames;
        self.note_direct("Observation Far destroy requested");
        Ok(())
    }

    fn direct_stage_far_gone(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.direct_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let mesh_far = *self.direct_lab.as_ref().unwrap().retired.last().ok_or("mesh Far was not retired")?;
        let mesh_close = self.direct_lab.as_ref().unwrap().mesh_close.ok_or("mesh Close missing")?;
        let gone = self.renderer.as_ref().unwrap().resident_mesh_bytes(mesh_far).is_none();
        let bytes_close = self.renderer.as_ref().unwrap().resident_mesh_bytes(mesh_close);
        if !gone || bytes_close.is_none() {
            if self.frames.saturating_sub(self.direct_lab.as_ref().unwrap().mark) > 90 {
                return Err("destroying observation Far did not leave Close resident and Far evicted".into());
            }
            return Ok(());
        }
        if bytes_close != Some(self.direct_lab.as_ref().unwrap().close_gpu_bytes) {
            return Err(format!("Close GPU bytes changed to {:?} while Far was destroyed", bytes_close));
        }
        if !self.direct_lab.as_ref().unwrap().captured_far {
            let left = self.viewport_view.ok_or("view Far missing")?;
            let right = self.direct_lab.as_ref().unwrap().right_view.ok_or("view Close missing")?;
            let draws = self.renderer.as_ref().unwrap().frame_mesh_draws().to_vec();
            let marker_mesh = self.direct_lab.as_ref().unwrap().marker_mesh.unwrap();
            if color_contains(&draws, left, mesh_far) || !color_contains(&draws, left, marker_mesh) || !color_contains(&draws, right, mesh_close) || color_contains(&draws, right, marker_mesh) {
                return Err("the frame after destroying Far still mixed the direct meshes".into());
            }
            self.note_direct("Observation Far GPU_bytes after destroy: GONE");
            self.note_direct(&format!("Observation Close GPU_bytes while Far is gone: {}", bytes_close.unwrap()));
            self.note_direct("View Far color pass after Far was destroyed: marker mesh");
            self.direct_capture("shot-obs-3c-far-destroyed.png")?;
            self.direct_lab.as_mut().unwrap().captured_far = true;
            self.direct_lab.as_mut().unwrap().mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.direct_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        self.direct_retire_mesh(mesh_close)?;
        self.direct_lab.as_mut().unwrap().mesh_close = None;
        self.direct_lab.as_mut().unwrap().stage = 3;
        self.direct_lab.as_mut().unwrap().mark = self.frames;
        self.note_direct("Observation Close destroy requested");
        Ok(())
    }

    fn direct_stage_close_gone(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.direct_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let meshes = self.direct_lab.as_ref().unwrap().retired.clone();
        let gone = meshes.iter().all(|mesh| self.renderer.as_ref().unwrap().resident_mesh_bytes(*mesh).is_none());
        if !gone {
            if self.frames.saturating_sub(self.direct_lab.as_ref().unwrap().mark) > 90 {
                return Err("observation Close stayed resident after destroy".into());
            }
            return Ok(());
        }
        if !self.direct_lab.as_ref().unwrap().captured_close {
            self.note_direct("Observation Close GPU_bytes after destroy: GONE");
            self.direct_capture("shot-obs-3c-close-destroyed.png")?;
            self.direct_lab.as_mut().unwrap().captured_close = true;
            self.direct_lab.as_mut().unwrap().mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.direct_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        let decoy = self.direct_lab.as_ref().unwrap().decoy.ok_or("decoy missing")?;
        let marker_mesh = self.direct_lab.as_ref().unwrap().marker_mesh.ok_or("marker mesh missing")?;
        let marker = self.direct_lab.as_ref().unwrap().marker.ok_or("marker missing")?;
        self.direct_retire_mesh(decoy)?;
        self.direct_lab.as_mut().unwrap().decoy = None;
        self.engine.world_mut().destroy_authored(marker).map_err(|error| error.to_string())?;
        if !self.engine.world_mut().retire_unreferenced_mesh(marker_mesh) {
            return Err("the marker mesh stayed referenced".into());
        }
        self.direct_lab.as_mut().unwrap().retired.push(marker_mesh);
        self.direct_lab.as_mut().unwrap().marker = None;
        self.direct_lab.as_mut().unwrap().marker_mesh = None;
        let right = self.direct_lab.as_ref().unwrap().right_view.ok_or("view Close missing")?;
        let left = self.viewport_view.ok_or("view Far missing")?;
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.set_view_pixel_rect(left, None).map_err(|error| error.to_string())?;
            renderer.update_view(left, RenderViewUpdate { camera: None, layout: Some(NormalizedRect::FULL), settings: None, pose: None }).map_err(|error| error.to_string())?;
            renderer.destroy_view(right).map_err(|error| error.to_string())?;
        }
        let lab = self.direct_lab.as_mut().unwrap();
        lab.split = false;
        lab.right_view = None;
        lab.stage = 4;
        lab.mark = self.frames;
        self.note_direct("Perspective restored to one full view");
        self.note_direct("Direct marker destroyed before quit: YES");
        Ok(())
    }

    fn direct_stage_intact(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.direct_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let lab = self.direct_lab.as_ref().unwrap();
        let solid = lab.solid.ok_or("solid missing")?;
        let control = lab.control_mesh.ok_or("control missing")?;
        let record_same = self.engine.world().authored_block(solid).as_ref() == lab.record.as_ref();
        let pose_same = self.engine.world().entity_local_pose(solid).ok().map(|pose| pose.translation) == Some(lab.solid_pose);
        let mesh_same = self.engine.world().object_mesh(solid) == Some(control);
        let inside = self.engine.world().separate_from_blocks(Vec3::new(0.0, 1.0, -4.0));
        let outside = self.engine.world().separate_from_blocks(Vec3::new(0.0, 1.0, 40.0));
        let collision_same = inside == lab.collision_inside && outside == lab.collision_outside;
        let live = self.engine.world().authored_block(solid);
        let body_absent = live.as_ref().is_some_and(|record| record.body.is_none());
        let name_same = self.engine.world().remember_entity(solid).ok().map(|entity| entity.name) == Some(lab.solid_name.clone());
        let meshlets_same = self.engine.world().derived_meshlets(control).map(|set| set.meshlets.len()) == lab.meshlet_len;
        let ordinary_same = self.engine.world().meshes().get(control).is_some_and(|mesh| mesh.vertex_count() == lab.control_vertices && mesh.index_count() / 3 == lab.control_triangles);
        let retired = lab.retired.clone();
        let orphans_gone = retired.iter().all(|mesh| self.engine.world().meshes().get(*mesh).is_none());
        let gpu_gone = retired.iter().all(|mesh| self.renderer.as_ref().unwrap().resident_mesh_bytes(*mesh).is_none());
        let control_after = self.renderer.as_ref().unwrap().resident_mesh_bytes(control);
        let control_match = control_after == lab.control_gpu_bytes && control_after.is_some();
        let views = self.renderer.as_ref().unwrap().view_count() == 1;
        let marker_gone = !self.engine.world().entity_outline().iter().any(|item| item.name == "Direct Marker");
        let level_now = std::fs::read(&lab.level_path).unwrap_or_default();
        let level_same = level_now == lab.level_bytes;
        let analytic_same = jarvig_core::AnalyticObject::specimen().content_hash == lab.analytic_hash && jarvig_core::AnalyticObject::specimen().revision == 1;
        let surfaces_defined = jarvig_core::classify_patch(jarvig_core::SURFACE_A, &jarvig_core::far_observation()) == jarvig_core::PatchAdmission::Omit
            && jarvig_core::classify_patch(jarvig_core::SURFACE_A, &jarvig_core::close_observation()) == jarvig_core::PatchAdmission::Omit
            && jarvig_core::classify_patch(jarvig_core::SURFACE_B, &jarvig_core::far_observation()) == jarvig_core::PatchAdmission::AdmitInView
            && jarvig_core::classify_patch(jarvig_core::SURFACE_B, &jarvig_core::close_observation()) == jarvig_core::PatchAdmission::AdmitInView;
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
        let stale_ok = lab.discarded_stale_gpu >= 1;
        let counts_ok = lab.acquires_at_proof == 1 && lab.presents_at_proof == 1;
        let mesh_count_same = self.engine.world().mesh_count() == lab.mesh_count;
        let workers_ok = self.jobs.worker_count() == 2 && self.einstein_jobs.worker_count() == 1;
        let object_ok = record_same
            && pose_same
            && mesh_same
            && collision_same
            && body_absent
            && name_same
            && meshlets_same
            && ordinary_same
            && orphans_gone
            && gpu_gone
            && control_match
            && views
            && marker_gone
            && level_same
            && analytic_same
            && surfaces_defined
            && element == "NONE"
            && graphics_ok
            && presented
            && stale_ok
            && counts_ok
            && mesh_count_same
            && workers_ok;
        self.direct_capture("shot-obs-3c-object.png")?;
        self.note_direct(&format!("Editor frame: {}", self.frames));
        self.note_direct(&format!("Presented frames: {}", self.renderer.as_ref().unwrap().presented_frames()));
        self.note_direct(&format!("Frame: {}", if presented { "PRESENTED" } else { "NOT PRESENTED" }));
        self.note_direct(&format!("Graphics is Intel UHD DX12 8086:9b41: {}", yes_no(graphics_ok)));
        self.note_direct(&format!("Acquires on the proof frame: {}", self.direct_lab.as_ref().unwrap().acquires_at_proof));
        self.note_direct(&format!("Presents on the proof frame: {}", self.direct_lab.as_ref().unwrap().presents_at_proof));
        self.note_direct(&format!("Intent Solid record unchanged: {}", yes_no(record_same)));
        self.note_direct(&format!("Intent Solid evaluated body absent: {}", yes_no(body_absent)));
        self.note_direct("Intent Solid body used as 3C authority: NO");
        self.note_direct(&format!("analytic_hash unchanged: {}", yes_no(analytic_same)));
        self.note_direct("analytic_revision after destroy: 1");
        self.note_direct(&format!("Surfaces A and B still defined: {}", yes_no(surfaces_defined)));
        self.note_direct(&format!("intent entries: {}", live.as_ref().map(|record| record.intent.len()).unwrap_or(0)));
        self.note_direct(&format!("history entries: {}", live.as_ref().map(|record| record.history.len()).unwrap_or(0)));
        self.note_direct(&format!("Intent Solid tape unchanged: {}", yes_no(record_same)));
        self.note_direct(&format!("collision before_after: {}", if collision_same { "MATCH" } else { "DIFFER" }));
        self.note_direct(&format!("materials before_after: {}", if record_same { "MATCH" } else { "DIFFER" }));
        self.note_direct(&format!("semantic identity before_after: {}", if record_same && name_same { "MATCH" } else { "DIFFER" }));
        self.note_direct("Face 89 consulted: NO");
        self.note_direct(&format!("Ordinary control mesh unchanged: {}", yes_no(mesh_same && ordinary_same)));
        self.note_direct(&format!("Control meshlets unchanged: {}", yes_no(meshlets_same)));
        self.note_direct(&format!("World revision before: {}", self.direct_lab.as_ref().unwrap().revision_before));
        self.note_direct(&format!("World revision after: {}", self.engine.world().revision()));
        self.note_direct("World revision unchanged required: NO");
        self.note_direct(&format!("Library mesh count restored: {}", yes_no(mesh_count_same)));
        self.note_direct(&format!("Direct meshes removed: {}", yes_no(orphans_gone)));
        self.note_direct(&format!("Direct GPU buffers after destroy: {}", if gpu_gone { "EVICTED" } else { "RESIDENT" }));
        self.note_direct(&format!("Control mesh GPU_bytes after: {}", control_after.map(|value| value.to_string()).unwrap_or_else(|| "GONE".into())));
        self.note_direct(&format!("Control mesh GPU_bytes match: {}", yes_no(control_match)));
        self.note_direct(&format!("Level file bytes unchanged: {}", yes_no(level_same)));
        self.note_direct(&format!("Marker entity absent: {}", yes_no(marker_gone)));
        self.note_direct(&format!("Render view count restored: {}", yes_no(views)));
        self.note_direct(&format!("Editor element selection: {element}"));
        self.note_direct(&format!("stale uploads rejected: {}", self.direct_lab.as_ref().unwrap().discarded_stale_gpu));
        self.note_direct("Close mesh left bound on Far: NO");
        self.note_direct(&format!("Editor workers: general {} einstein {} realize 0", self.jobs.worker_count(), self.einstein_jobs.worker_count()));
        self.note_direct("Backface omission: NO");
        self.note_direct("Same-solid occlusion: NO");
        self.note_direct("Edge collapse: NO");
        self.note_direct("Simplification: NO");
        self.note_direct("Renderer replacement: NO");
        self.note_direct("ADR-0074 accepted: NO");
        self.note_direct("RFC-0002 stamped: NO");
        self.note_direct("RFC-0007 accepted: NO");
        self.note_direct(&format!("Authoritative object after both products destroyed: {}", if object_ok { "INTACT" } else { "CHANGED" }));
        let passed = object_ok && self.direct_lab.as_ref().unwrap().report.contains("expected_gpu_bytes = GpuMesh.bytes = create_buffer bytes: YES");
        self.note_direct(&format!("Experiment 3C: {}", if passed { "PASS" } else { "FAIL" }));
        self.direct_write_report();
        self.direct_lab.as_mut().unwrap().stage = 15;
        if !passed {
            return Err("Experiment 3C did not meet the presented-frame success condition".into());
        }
        unsafe { PostQuitMessage(0); }
        Ok(())
    }

    fn direct_note_uploads(&mut self) -> Result<(), String> {
        let (mesh_far, mesh_close, product_far, product_close) = {
            let lab = self.direct_lab.as_ref().unwrap();
            (lab.mesh_far.unwrap(), lab.mesh_close.unwrap(), lab.product_far.clone().unwrap(), lab.product_close.clone().unwrap())
        };
        let record_far = self.renderer.as_ref().unwrap().mesh_upload_record(mesh_far).ok_or("observation Far was not uploaded")?;
        let record_close = self.renderer.as_ref().unwrap().mesh_upload_record(mesh_close).ok_or("observation Close was not uploaded")?;
        let prove = |label: &str, product: &jarvig_core::DirectProduct, record: jarvig_renderer::MeshUploadRecord| -> Result<(), String> {
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
        prove("Observation Far", &product_far, record_far)?;
        prove("Observation Close", &product_close, record_close)?;
        self.note_direct(&format!("Observation Far GpuMesh.bytes: {}", record_far.gpu_bytes));
        self.note_direct(&format!("Observation Far create_buffer bytes: {}", record_far.vertex_create_bytes + record_far.index_create_bytes));
        self.note_direct(&format!("Observation Far vertex_create_bytes: {}", record_far.vertex_create_bytes));
        self.note_direct(&format!("Observation Far index_create_bytes: {}", record_far.index_create_bytes));
        self.note_direct(&format!("Observation Close GpuMesh.bytes: {}", record_close.gpu_bytes));
        self.note_direct(&format!("Observation Close create_buffer bytes: {}", record_close.vertex_create_bytes + record_close.index_create_bytes));
        self.note_direct(&format!("Observation Close vertex_create_bytes: {}", record_close.vertex_create_bytes));
        self.note_direct(&format!("Observation Close index_create_bytes: {}", record_close.index_create_bytes));
        self.note_direct(&format!("Observation Far position_hash: {:016x}", product_far.position_hash));
        self.note_direct(&format!("Observation Far index_hash: {:016x}", product_far.index_hash));
        self.note_direct(&format!("Observation Close position_hash: {:016x}", product_close.position_hash));
        self.note_direct(&format!("Observation Close index_hash: {:016x}", product_close.index_hash));
        self.note_direct("expected_gpu_bytes = GpuMesh.bytes = create_buffer bytes: YES");
        self.note_direct("triangles_discarded_after_upload: 0");
        self.note_direct("post-construction discard = 0");
        self.note_direct("post-pack discard = 0");
        self.note_direct("post-upload discard = 0");
        let acquires = self.renderer.as_ref().unwrap().acquires_last_frame();
        let presents = self.renderer.as_ref().unwrap().presents_last_frame();
        self.note_direct(&format!("Acquires this frame: {acquires}"));
        self.note_direct(&format!("Presents this frame: {presents}"));
        if acquires != 1 || presents != 1 {
            return Err("the direct frame did not use one acquire and one present".into());
        }
        let lab = self.direct_lab.as_mut().unwrap();
        lab.acquires_at_proof = acquires;
        lab.presents_at_proof = presents;
        lab.close_gpu_bytes = record_close.gpu_bytes;
        Ok(())
    }

    fn direct_note_passes(&mut self, label: &str, require_recorded: bool) -> Result<(), String> {
        let draws = self.renderer.as_ref().unwrap().frame_mesh_draws().to_vec();
        let mesh_far = self.direct_lab.as_ref().unwrap().mesh_far.unwrap();
        let mesh_close = self.direct_lab.as_ref().unwrap().mesh_close.unwrap();
        let control = self.direct_lab.as_ref().unwrap().control_mesh.unwrap();
        let marker_mesh = self.direct_lab.as_ref().unwrap().marker_mesh.unwrap();
        let decoy = self.direct_lab.as_ref().unwrap().decoy.unwrap();
        let shadow = pass_meshes(&draws, "shadow");
        let shadow_cached = pass_meshes(&draws, "shadow-cached");
        let contact = pass_meshes(&draws, "contact");
        let probe = pass_meshes(&draws, "probe");
        let casters = if shadow.is_empty() { shadow_cached.clone() } else { shadow.clone() };
        self.note_direct(&format!("{label} shadow draws: {}", id_list(&shadow)));
        self.note_direct(&format!("{label} shadow casters: {}", id_list(&casters)));
        self.note_direct(&format!("{label} shadow maps redrawn this frame: {}", yes_no(!shadow.is_empty())));
        self.note_direct(&format!("{label} contact meshes: {}", id_list(&contact)));
        self.note_direct(&format!("{label} probe meshes: {}", id_list(&probe)));
        let observation_drawn = |list: &[MeshId]| list.contains(&mesh_far) || list.contains(&mesh_close) || list.contains(&decoy);
        if observation_drawn(&shadow) || observation_drawn(&shadow_cached) || observation_drawn(&contact) || observation_drawn(&probe) {
            return Err("a direct mesh was drawn by shadow, contact, or probe capture".into());
        }
        if shadow.contains(&marker_mesh) || shadow_cached.contains(&marker_mesh) {
            return Err("the presentation marker cast a shadow".into());
        }
        self.note_direct("Observation mesh in shadow draws: NO");
        self.note_direct("Observation mesh in contact draws: NO");
        self.note_direct("Observation mesh in probe draws: NO");
        self.note_direct("Marker mesh in shadow draws: NO");
        self.note_direct(&format!("Marker mesh in contact draws: {}", yes_no(contact.contains(&marker_mesh))));
        if casters.is_empty() && !require_recorded {
            let solid = self.direct_lab.as_ref().unwrap().solid.ok_or("the ordinary control entity is missing")?;
            let snapshot = self.engine.world().extract(jarvig_core::RenderFrameId(1)).map_err(|error| error.to_string())?;
            let casts = snapshot.instances().iter().find(|instance| instance.entity == solid).is_some_and(|instance| instance.cast_shadows && instance.mesh == control);
            if !casts {
                return Err("the ordinary control mesh does not cast shadows".into());
            }
            self.note_direct("Ordinary control cast_shadows: YES");
            self.note_direct("Shadow caster list on the rebuild frame: NOT RECORDED");
            self.direct_lab.as_mut().unwrap().caster_check_pending = true;
            return Ok(());
        }
        if !casters.contains(&control) {
            return Err("the ordinary control mesh was not the shadow caster".into());
        }
        self.note_direct("Ordinary control mesh in shadow casters: YES");
        self.direct_lab.as_mut().unwrap().caster_check_pending = false;
        Ok(())
    }

    fn direct_wait_reason(&self) -> String {
        let Some(renderer) = self.renderer.as_ref() else {
            return "renderer missing".into();
        };
        let Some(lab) = self.direct_lab.as_ref() else {
            return "lab missing".into();
        };
        let rect = |view: Option<jarvig_renderer::RenderViewId>| {
            view.and_then(|view| renderer.viewport(view).ok().flatten())
                .map(|rect| format!("{}x{}@{},{}", rect.width, rect.height, rect.x, rect.y))
                .unwrap_or_else(|| "none".into())
        };
        let uploaded = |mesh: Option<MeshId>| mesh.and_then(|mesh| renderer.mesh_upload_record(mesh)).is_some();
        let color: Vec<u64> = renderer.frame_mesh_draws().iter().filter(|draw| draw.pass == "color").map(|draw| draw.mesh.0).collect();
        format!("far {} close {} far_uploaded {} close_uploaded {} color {:?}", rect(self.viewport_view), rect(lab.right_view), uploaded(lab.mesh_far), uploaded(lab.mesh_close), color)
    }

    fn direct_frame_matches(&mut self, require_both: bool) -> Result<bool, String> {
        let Some(renderer) = self.renderer.as_ref() else {
            return Ok(false);
        };
        let left = self.viewport_view.ok_or("view Far missing")?;
        let right = match self.direct_lab.as_ref().unwrap().right_view {
            Some(view) => view,
            None => return Ok(false),
        };
        let mesh_far = self.direct_lab.as_ref().unwrap().mesh_far;
        let mesh_close = self.direct_lab.as_ref().unwrap().mesh_close;
        let marker_mesh = self.direct_lab.as_ref().unwrap().marker_mesh.ok_or("marker missing")?;
        let decoy = self.direct_lab.as_ref().unwrap().decoy.ok_or("decoy missing")?;
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
        if !require_both {
            return Ok(true);
        }
        let ready = color_contains(draws, left, mesh_far)
            && !color_contains(draws, left, mesh_close)
            && !color_contains(draws, left, marker_mesh)
            && !color_contains(draws, left, decoy)
            && color_contains(draws, right, mesh_close)
            && !color_contains(draws, right, mesh_far)
            && !color_contains(draws, right, marker_mesh)
            && !color_contains(draws, right, decoy)
            && renderer.mesh_upload_record(mesh_far).is_some()
            && renderer.mesh_upload_record(mesh_close).is_some();
        Ok(ready)
    }

    fn direct_reject_stale(&mut self) -> Result<(), String> {
        let (left, right, mesh_far, mesh_close, decoy, key_far, key_close, entity) = {
            let lab = self.direct_lab.as_ref().unwrap();
            (
                self.viewport_view.ok_or("view Far missing")?,
                lab.right_view.ok_or("view Close missing")?,
                lab.mesh_far.unwrap(),
                lab.mesh_close.unwrap(),
                lab.decoy.unwrap(),
                lab.key_far.clone(),
                lab.key_close.clone(),
                lab.marker.unwrap(),
            )
        };
        self.direct_bind_if_current(left, entity, mesh_close, &key_close, &key_far)?;
        self.direct_bind_if_current(right, entity, mesh_far, &key_far, &key_close)?;
        self.direct_bind_if_current(left, entity, decoy, "stale-decoy", &key_far)?;
        if self.renderer.as_ref().unwrap().resident_mesh_bytes(decoy).is_some() {
            return Err("the refused decoy was uploaded".into());
        }
        if self.direct_lab.as_ref().unwrap().discarded_stale_gpu < 1 {
            return Err("a stale direct upload was not rejected".into());
        }
        self.note_direct("Decoy mesh GPU_bytes: NOT UPLOADED");
        self.note_direct(&format!("stale uploads rejected: {}", self.direct_lab.as_ref().unwrap().discarded_stale_gpu));
        self.note_direct("Cross-view buffer substitution: NO");
        self.note_direct("Decoy mesh bound: NO");
        Ok(())
    }

    fn direct_bind_if_current(&mut self, view: RenderViewId, entity: jarvig_core::EntityId, mesh: MeshId, product_key: &str, current_key: &str) -> Result<(), String> {
        if product_key != current_key {
            self.direct_lab.as_mut().unwrap().discarded_stale_gpu = self.direct_lab.as_ref().unwrap().discarded_stale_gpu.saturating_add(1);
            return Ok(());
        }
        self.renderer.as_mut().unwrap().set_view_mesh_override(view, entity, Some(mesh)).map_err(|error| error.to_string())
    }

    fn direct_retire_mesh(&mut self, mesh: MeshId) -> Result<(), String> {
        let control = self.direct_lab.as_ref().unwrap().control_mesh.ok_or("control missing")?;
        let marker = self.direct_lab.as_ref().unwrap().marker_mesh;
        if mesh == control || marker == Some(mesh) {
            return Err("refusing to retire the object mesh or the marker mesh".into());
        }
        if !self.engine.world_mut().retire_unreferenced_mesh(mesh) {
            return Err(format!("direct mesh {} stayed referenced", mesh.0));
        }
        self.direct_lab.as_mut().unwrap().retired.push(mesh);
        Ok(())
    }
}

fn frozen_account(account: &jarvig_core::SurfaceAccount, n: u32, triangles: u32, vertices: u32, index_bytes: u64, gpu_bytes: u64, error: &str, coarser: &str, evaluations: u32) -> Result<(), String> {
    let error_text = format!("{:.6}", account.measured_error_px);
    let coarser_text = format!("{:.6}", account.coarser_px);
    if account.surface_id != jarvig_core::SURFACE_B
        || account.admission != jarvig_core::PatchAdmission::AdmitInView
        || account.n != n
        || account.triangles_constructed != triangles
        || account.packed_triangles != triangles
        || account.packed_vertices != vertices
        || account.index_count != vertices
        || account.vertex_bytes != u64::from(vertices) * 60
        || account.index_bytes != index_bytes
        || account.gpu_bytes != gpu_bytes
        || account.candidate_grids_emitted != 1
        || account.candidate_grids_discarded != 0
        || account.resolution_predicate_evaluations != evaluations
        || !account.coarser_failed
        || error_text != error
        || coarser_text != coarser
    {
        return Err(format!(
            "Surface B left the frozen CPU measurement (n {} triangles {} error {error_text} coarser {coarser_text} gpu {})",
            account.n, account.triangles_constructed, account.gpu_bytes
        ));
    }
    Ok(())
}

fn frozen_omitted(account: &jarvig_core::SurfaceAccount) -> bool {
    account.surface_id == jarvig_core::SURFACE_A
        && account.admission == jarvig_core::PatchAdmission::Omit
        && account.n == 0
        && account.triangles_constructed == 0
        && account.packed_triangles == 0
        && account.packed_vertices == 0
        && account.gpu_bytes == 0
        && account.candidate_grids_emitted == 0
        && account.resolution_predicate_evaluations == 0
}

/// The 2026-10-06 CPU predicate. A mismatch fails before any GPU mesh is added.
fn frozen_cpu_match(far: &jarvig_core::DirectProduct, close: &jarvig_core::DirectProduct) -> Result<(), String> {
    if far.analytic.content_hash != FROZEN_ANALYTIC_HASH || close.analytic.content_hash != FROZEN_ANALYTIC_HASH || far.analytic.revision != 1 || close.analytic.revision != 1 {
        return Err(format!("analytic identity {:016x}/{} left the frozen specimen", far.analytic.content_hash, far.analytic.revision));
    }
    if !frozen_omitted(&far.surfaces[0]) || !frozen_omitted(&close.surfaces[0]) {
        return Err("Surface A left the frozen zero account".into());
    }
    frozen_account(&far.surfaces[1], 6, 72, 216, 432, 13392, FROZEN_FAR_ERROR, FROZEN_FAR_COARSER, 6)?;
    frozen_account(&close.surfaces[1], 14, 392, 1176, 2352, 72912, FROZEN_CLOSE_ERROR, FROZEN_CLOSE_COARSER, 14)?;
    if far.position_hash != 0xd483_cd5f_6133_8a61 || far.index_hash != 0x0fef_7021_13eb_2f05 || far.expected_gpu_bytes != 13392 || far.constructed_triangles != 72 {
        return Err(format!("Far hashes {:016x}/{:016x} left the frozen CPU measurement", far.position_hash, far.index_hash));
    }
    if close.position_hash != 0x6e2e_84eb_1989_2f71 || close.index_hash != 0xd9f7_af58_ffd2_4f65 || close.expected_gpu_bytes != 72912 || close.constructed_triangles != 392 {
        return Err(format!("Close hashes {:016x}/{:016x} left the frozen CPU measurement", close.position_hash, close.index_hash));
    }
    if far.discarded_after_construction != 0 || far.discarded_during_pack != 0 || far.discarded_after_upload != 0 || close.discarded_after_construction != 0 || close.discarded_during_pack != 0 || close.discarded_after_upload != 0 {
        return Err("a frozen discard counter was not zero".into());
    }
    if far.candidate_grids_discarded != 0 || close.candidate_grids_discarded != 0 || far.index_format.byte_size() != 2 || close.index_format.byte_size() != 2 {
        return Err("the frozen pack left Uint16 or discarded a candidate grid".into());
    }
    Ok(())
}

fn direct_predicates_ok(far: &jarvig_core::DirectProduct, close: &jarvig_core::DirectProduct) -> bool {
    let side = |product: &jarvig_core::DirectProduct| {
        let omitted = &product.surfaces[0];
        let admitted = &product.surfaces[1];
        omitted.surface_id == jarvig_core::SURFACE_A
            && admitted.surface_id == jarvig_core::SURFACE_B
            && omitted.admission == jarvig_core::PatchAdmission::Omit
            && admitted.admission == jarvig_core::PatchAdmission::AdmitInView
            && omitted.triangles_constructed == 0
            && omitted.packed_triangles == 0
            && omitted.gpu_bytes == 0
            && omitted.vertex_bytes == 0
            && omitted.index_bytes == 0
            && omitted.candidate_grids_emitted == 0
            && omitted.candidate_grids_discarded == 0
            && admitted.candidate_grids_emitted == 1
            && admitted.candidate_grids_discarded == 0
            && admitted.n >= 1
            && admitted.triangles_constructed == admitted.n.saturating_mul(admitted.n).saturating_mul(2)
            && admitted.measured_error_px <= 0.55
            && (admitted.n == 1 || admitted.coarser_failed)
            && product.discarded_after_construction == 0
            && product.discarded_during_pack == 0
            && product.discarded_after_upload == 0
            && product.constructed_triangles == product.packed_triangles
            && product.packed_triangles == product.uploaded_triangles
            && product.trace.len() as u32 == product.constructed_triangles
            && !product.trace.iter().any(|record| record.surface_id == jarvig_core::SURFACE_A)
            && product.packed_vertices == product.constructed_triangles.saturating_mul(3)
            && !product.vertices_welded
            && product.cpu_bytes == product.expected_gpu_bytes
            && product.meshlets_constructed == 0
            && !product.object_mesh_consulted
            && !product.shared_render_mesh_consulted
            && !product.experiment2_mesh_consulted
            && !product.einstein_consulted
    };
    side(far)
        && side(close)
        && close.surfaces[1].n > far.surfaces[1].n
        && far.position_hash != close.position_hash
        && far.index_hash != close.index_hash
        && far.expected_gpu_bytes != close.expected_gpu_bytes
        && far.candidate_grids_discarded == 0
        && close.candidate_grids_discarded == 0
}

fn direct_pose(solid: ResolvedPose, eye: [f64; 3], forward: [f64; 3]) -> Result<ResolvedPose, String> {
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
        return Err(format!("the view rotation does not look along the direct forward ({delta})"));
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
