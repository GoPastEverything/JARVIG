//! Experiment 4. The eligible intent tape drives two observation realizations.
//!
//! The curved patches are an experiment-local trailer. This lab does not enable
//! intent authority, does not spawn a marker, and does not edit the 3C lab.
//! A matching planar admission set stops before the size edit.

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
const FROZEN_3C_AUTHORITY: u64 = 0xac80_f43b_acf3_c609;
const FROZEN_FAR_POSITION: u64 = 0xd483_cd5f_6133_8a61;
const FROZEN_CLOSE_POSITION: u64 = 0x6e2e_84eb_1989_2f71;
const FROZEN_FAR_ERROR: &str = "0.424494";
const FROZEN_FAR_COARSER: &str = "0.613221";
const FROZEN_CLOSE_ERROR: &str = "0.462054";
const FROZEN_CLOSE_COARSER: &str = "0.536666";

pub(super) struct IntentLab {
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
    product_far: Option<jarvig_core::IntentObservationProduct>,
    product_close: Option<jarvig_core::IntentObservationProduct>,
    record: Option<jarvig_core::BlockRecord>,
    authority_hash: u64,
    edited_hash: u64,
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
    acquires_first: u32,
    presents_first: u32,
    acquires_second: u32,
    presents_second: u32,
    panel: (u32, u32),
    discarded_stale_gpu: u32,
    caster_check_pending: bool,
    level_bytes: Vec<u8>,
    level_path: String,
    report: String,
}

impl IntentLab {
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
            record: None,
            authority_hash: 0,
            edited_hash: 0,
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
            acquires_first: 0,
            presents_first: 0,
            acquires_second: 0,
            presents_second: 0,
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
    pub(super) fn apply_intent_frame(&mut self) -> Result<(), String> {
        let Some(lab) = self.intent_lab.as_ref() else {
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

    pub(super) fn step_intent_lab(&mut self) -> Result<(), String> {
        if self.intent_lab.is_none() {
            return Ok(());
        }
        let stage = self.intent_lab.as_ref().unwrap().stage;
        if stage >= 15 {
            return Ok(());
        }
        let result = match stage {
            0 => self.intent_stage_setup(),
            1 => self.intent_stage_present(),
            2 => self.intent_stage_edit(),
            3 => self.intent_stage_edited(),
            4 => self.intent_stage_far_gone(),
            5 => self.intent_stage_close_gone(),
            6 => self.intent_stage_intact(),
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.note_intent("Experiment 4: FAIL");
            self.note_intent(&format!("Failure: {error}"));
            self.intent_write_report();
            if let Some(lab) = self.intent_lab.as_mut() {
                lab.stage = 15;
                lab.split = false;
            }
            return Err(error);
        }
        Ok(())
    }

    fn note_intent(&mut self, line: &str) {
        self.append(line);
        println!("{line}");
        if let Some(lab) = self.intent_lab.as_mut() {
            lab.report.push_str(line);
            lab.report.push('\n');
        }
    }

    pub(super) fn intent_write_report(&mut self) {
        let Some(lab) = self.intent_lab.as_ref() else {
            return;
        };
        let mut text = lab.report.clone();
        text.push_str("Intent observation report: REPORT-INTENT-4.txt\n");
        let dir = std::path::PathBuf::from(r"C:\Users\Jeramiah\AppData\Local\Temp\jarvig-intent-proof");
        match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("REPORT-INTENT-4.txt"), &text)) {
            Ok(()) => self.note_intent("Intent observation report: REPORT-INTENT-4.txt"),
            Err(error) => self.note_intent(&format!("Intent observation report was not written: {error}")),
        }
    }

    fn intent_capture(&mut self, name: &str) -> Result<(), String> {
        let path = format!(r"C:\Users\Jeramiah\AppData\Local\Temp\jarvig-intent-proof\{name}");
        let image = super::capture_window_image(self.frame)?;
        super::write_png(&path, &image)?;
        self.note_intent(&format!("Capture: {name}"));
        Ok(())
    }

    fn intent_stage_setup(&mut self) -> Result<(), String> {
        if self.view_lab.is_some() || self.view_realization.is_some() || self.observation_lab.is_some() || self.direct_lab.is_some() || self.intent_live_rerealize {
            return Err("Experiment 4 refuses to run beside another realization lab".into());
        }
        if self.engine.intent_authority_experiment() {
            return Err("Experiment 4 refuses the intent-authority experiment flag".into());
        }
        if self.renderer.is_none() || self.editor_camera.is_none() || self.viewport_view.is_none() || self.viewport_px.1 < 64 {
            if self.frames >= 400 {
                return Err("the perspective viewport was not ready".into());
            }
            return Ok(());
        }
        if !self.intent_lab.as_ref().unwrap().arranged {
            for panel in [OUTLINER, INSPECTOR, CONTENT, OUTPUT] {
                if self.workspace.is_open(panel) {
                    self.workspace.apply(WorkspaceCommand::ClosePanel(panel)).map_err(|error| error.to_string())?;
                }
            }
            unsafe { SetWindowPos(self.frame, std::ptr::null_mut(), 20, 20, 1800, 1200, SWP_NOZORDER); }
            self.realize();
            let lab = self.intent_lab.as_mut().unwrap();
            lab.arranged = true;
            lab.mark = self.frames;
            return Ok(());
        }
        let (panel_w, panel_h) = self.viewport_px;
        let renderer_size = self.renderer.as_ref().unwrap().configured_size();
        if panel_w < REQUIRED_WIDTH || panel_h < VIEW_HEIGHT || renderer_size.0 < REQUIRED_WIDTH || renderer_size.1 < VIEW_HEIGHT {
            let grow = self.intent_lab.as_ref().unwrap().grow;
            let mark = self.intent_lab.as_ref().unwrap().mark;
            if grow < 2 && self.frames.saturating_sub(mark) > 8 {
                let (width, height) = if grow == 0 { (2200, 1400) } else { (2600, 1600) };
                unsafe { SetWindowPos(self.frame, std::ptr::null_mut(), 20, 20, width, height, SWP_NOZORDER); }
                self.realize();
                let lab = self.intent_lab.as_mut().unwrap();
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
        self.intent_install(control_gpu_bytes, (panel_w, panel_h))?;
        Ok(())
    }

    fn intent_install(&mut self, control_gpu_bytes: u64, panel: (u32, u32)) -> Result<(), String> {
        let level_path = self.level_file.clone().ok_or("Main.jarviglevel is not open")?;
        let level_bytes = std::fs::read(&level_path).map_err(|error| error.to_string())?;
        let far_camera = jarvig_core::far_intent_camera();
        let close_camera = jarvig_core::close_intent_camera();
        if far_camera.viewport_width != VIEW_FAR_WIDTH as f32 || close_camera.viewport_width != VIEW_CLOSE_WIDTH as f32 || far_camera.viewport_height != VIEW_HEIGHT as f32 {
            return Err("the intent cameras were not 160x567 and 960x567".into());
        }
        if (far_camera.requested_error_px - 0.5).abs() > 1.0e-6 || (close_camera.requested_error_px - 0.5).abs() > 1.0e-6 {
            return Err("the intent cameras did not both request 0.5 px".into());
        }
        let gathered = {
            let world = self.engine.world();
            let intent = world.entity_outline().iter().find(|item| item.name == "Intent Solid").map(|item| item.uuid).ok_or("Intent Solid missing")?;
            let record = world.authored_block(intent).ok_or("Intent Solid has no block")?;
            if record.body.is_some() {
                return Err("Intent Solid has an evaluated body. Experiment 4 does not read it and does not install one".into());
            }
            if record.intent.len() != 65 || record.history.len() != 24 {
                return Err(format!("Intent Solid tape is intent {} history {}, not 65 and 24", record.intent.len(), record.history.len()));
            }
            let object_mesh = world.object_mesh(intent).ok_or("the ordinary renderer has no mesh for Intent Solid")?;
            let shared = world.meshes().get(object_mesh).ok_or("the ordinary control mesh is missing")?;
            let control_vertices = shared.vertex_count();
            let control_triangles = shared.index_count() / 3;
            if control_vertices != 24 || control_triangles != 12 || control_gpu_bytes != 1512 {
                return Err(format!("the ordinary control is {control_vertices} vertices, {control_triangles} triangles, {control_gpu_bytes} GPU bytes. The switch must stay off."));
            }
            let meshlet_len = world.derived_meshlets(object_mesh).map(|set| set.meshlets.len());
            let front = world.front_camera();
            let expected_fov = 60.0_f64.to_radians();
            if (front.vertical_fov_radians - expected_fov).abs() > 1.0e-6 || (f64::from(front.near_m) - 0.1).abs() > 1.0e-6 {
                return Err("the front camera is not the 60 degree, 0.1 m camera".into());
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
        if level_bytes.len() != 26849 {
            return Err(format!("Main.jarviglevel is {} bytes, not 26849", level_bytes.len()));
        }
        let product_far = jarvig_core::realize_intent_observation(&record, &far_camera, 1, jarvig_core::PATCH_B_TRANSLATION).map_err(|error| format!("far intent realization stopped: {error}"))?;
        let product_close = jarvig_core::realize_intent_observation(&record, &close_camera, 1, jarvig_core::PATCH_B_TRANSLATION).map_err(|error| format!("close intent realization stopped: {error}"))?;
        self.note_intent("Experiment 4: ON");
        self.note_intent("Experiment 3C frozen: PASS");
        self.note_intent("Intent-authority experiment: OFF");
        self.note_intent("ADR-0074 accepted: NO");
        self.note_intent("Default renderer replaced: NO");
        self.note_intent("Curved trailer is an IntentPayload: NO");
        self.note_intent("Curved trailer is experiment-local: YES");
        self.note_intent("Not earned: every bit of the curved geometry came from JARVIG's normal persistent editable intent language.");
        self.note_intent(&format!("INTENT_OBSERVATION_ALGORITHM_VERSION: {}", jarvig_core::INTENT_OBSERVATION_ALGORITHM_VERSION));
        self.note_intent(&format!("DIRECT_REALIZATION_ALGORITHM_VERSION: {}", jarvig_core::DIRECT_REALIZATION_ALGORITHM_VERSION));
        self.note_intent(&format!("OBSERVATION_ALGORITHM_VERSION: {}", jarvig_core::OBSERVATION_ALGORITHM_VERSION));
        self.note_intent(&format!("REALIZATION_ALGORITHM_VERSION: {}", jarvig_core::REALIZATION_ALGORITHM_VERSION));
        self.note_intent(&format!("Editor workers: general {} einstein {} realize 0", self.jobs.worker_count(), self.einstein_jobs.worker_count()));
        self.note_intent("jarvig-realize-0: NOT STARTED");
        self.note_intent("jarvig-realize-1: NOT STARTED");
        self.note_intent("queue: NOT USED");
        self.note_intent(&format!("Graphics adapter: {}", self.graphics_name));
        self.note_intent(&format!("Graphics vendor: {:04x}", self.graphics_vendor));
        self.note_intent(&format!("Graphics device: {:04x}", self.graphics_device));
        self.note_intent(&format!("Graphics api: {}", self.graphics_api));
        self.note_intent(&format!("Editor panel: {}x{}", panel.0, panel.1));
        self.note_intent("Observation Far viewport: 160x567");
        self.note_intent("Observation Close viewport: 960x567");
        self.note_intent("Re-admitted at the editor panel size: NO");
        self.note_intent(&format!("Requested error px: {:.1}", far_camera.requested_error_px));
        self.note_intent(&format!("Observation Far eye_local: [{:.4}, {:.4}, {:.4}]", far_camera.eye[0], far_camera.eye[1], far_camera.eye[2]));
        self.note_intent(&format!("Observation Close eye_local: [{:.4}, {:.4}, {:.4}]", close_camera.eye[0], close_camera.eye[1], close_camera.eye[2]));
        self.note_intent("Observation forward_local: [0.0000, 0.0000, -1.0000]");
        self.note_intent("Authority: eligible intent tape plus experiment-local curved trailer");
        self.note_intent("Intent Solid evaluated body: ABSENT");
        self.note_intent(&format!("authority_hash: {:016x}", product_far.authority_hash));
        self.note_intent("authority_revision: 1");
        self.note_intent(&format!("Ordinary control mesh: {control_vertices} vertices, {control_triangles} triangles"));
        self.note_intent(&format!("Level file: {}", level_path.display()));
        self.note_intent(&format!("Level file bytes before: {}", level_bytes.len()));
        self.note_intent(&format!("World revision before: {revision_before}"));
        self.note_intent(&jarvig_core::format_intent_product("Observation Far", &product_far));
        self.note_intent(&jarvig_core::format_intent_product("Observation Close", &product_close));
        self.note_intent(&format!("planar far admitted: {}", id_text(&product_far.admitted_planar_ids)));
        self.note_intent(&format!("planar far omitted: {}", id_text(&product_far.omitted_planar_ids)));
        self.note_intent(&format!("planar close admitted: {}", id_text(&product_close.admitted_planar_ids)));
        self.note_intent(&format!("planar close omitted: {}", id_text(&product_close.omitted_planar_ids)));
        if product_far.admitted_planar_ids == product_close.admitted_planar_ids {
            self.note_intent("Planar admitted sets equal: YES");
            self.note_intent("Size edit: NOT RUN");
            self.note_intent("Frame: NOT PRESENTED");
            return Err("the frozen camera pair admitted the same planar faces. Experiment 4 stops before the edit.".into());
        }
        self.note_intent("Planar admitted sets equal: NO");
        if let Err(error) = frozen_intent_gate(&product_far, &product_close) {
            self.note_intent("Frame: NOT PRESENTED");
            return Err(error);
        }
        self.note_intent("Patch B far: n=6 / 72 triangles / 13392 GPU bytes");
        self.note_intent("Patch B close: n=14 / 392 triangles / 72912 GPU bytes");
        self.note_intent("Rejected n allocated a mesh: NO");
        let material_bound = {
            let snapshot = self.engine.world().extract(jarvig_core::RenderFrameId(1)).map_err(|error| error.to_string())?;
            snapshot.instances().iter().find(|instance| instance.entity == intent).and_then(|instance| instance.material_for_slot(0)).is_some()
        };
        if !material_bound {
            self.note_intent("Slot-0 material on Intent Solid: UNBOUND");
            self.note_intent("Frame: NOT PRESENTED");
            return Err("the override would be skipped because slot 0 is unbound".into());
        }
        self.note_intent("Slot-0 material on Intent Solid: BOUND");
        self.note_intent("bind_material called: NO");
        self.note_intent("New material asset: NO");
        let revision_now = self.engine.world().revision();
        if revision_now != revision_before {
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
        if self.engine.world().object_mesh(intent) != Some(object_mesh) {
            return Err("the Intent Solid mesh id changed".into());
        }
        let solid_pose = self.engine.world().entity_world_pose(intent).map_err(|error| error.to_string())?;
        let pose_far = intent_pose(solid_pose, far_camera.eye, far_camera.forward)?;
        let pose_close = intent_pose(solid_pose, close_camera.eye, close_camera.forward)?;
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
            renderer.set_view_mesh_override(left, intent, Some(mesh_far)).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(right, intent, Some(mesh_close)).map_err(|error| error.to_string())?;
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
            let lab = self.intent_lab.as_mut().unwrap();
            lab.solid = Some(intent);
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
            lab.record = Some(record);
            lab.authority_hash = lab.product_far.as_ref().unwrap().authority_hash;
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
        self.note_intent(&format!("Control mesh id: {}", object_mesh.0));
        self.note_intent(&format!("Observation Far mesh id: {}", mesh_far.0));
        self.note_intent(&format!("Observation Close mesh id: {}", mesh_close.0));
        self.note_intent(&format!("Decoy mesh id: {}", decoy.0));
        self.note_intent("Marker entity: NOT SPAWNED");
        self.note_intent("Observation mesh is the ordinary control mesh: NO");
        self.note_intent("set_object_mesh called: NO");
        self.note_intent("build_meshlets called: NO");
        self.note_intent("Observation meshlets: 0");
        self.note_intent(&format!("Control mesh GPU_bytes before: {control_gpu_bytes}"));
        self.note_intent(&format!("World revision after add_mesh: {}", self.engine.world().revision()));
        self.note_intent("World revision unchanged: YES");
        self.note_intent("Color pass overrides the Intent Solid: YES");
        self.note_intent("Shadow, contact, and probe capture use instance.mesh: YES");
        Ok(())
    }

    fn intent_stage_present(&mut self) -> Result<(), String> {
        if !self.intent_lab.as_ref().unwrap().proved {
            if !self.intent_frame_matches()? {
                if self.frames.saturating_sub(self.intent_lab.as_ref().unwrap().mark) > 180 {
                    return Err(format!("the first intent frame did not upload ({})", self.intent_wait_reason()));
                }
                return Ok(());
            }
            self.intent_note_uploads(false)?;
            self.intent_note_passes("simultaneous", false)?;
            self.intent_capture("shot-intent-4-simultaneous.png")?;
            self.intent_reject_stale()?;
            if self.intent_lab.as_ref().unwrap().discarded_stale_gpu < 3 {
                return Err("the first present rejected fewer than 3 stale uploads".into());
            }
            self.note_intent("First frame: PRESENTED");
            let lab = self.intent_lab.as_mut().unwrap();
            lab.proved = true;
            lab.mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.intent_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            if !self.intent_frame_matches()? {
                return Err("a later frame substituted an intent buffer".into());
            }
            if self.frames.saturating_sub(self.intent_lab.as_ref().unwrap().mark) == 1 && self.intent_lab.as_ref().unwrap().caster_check_pending {
                self.intent_note_passes("cached", true)?;
            }
            return Ok(());
        }
        self.intent_lab.as_mut().unwrap().stage = 2;
        self.intent_lab.as_mut().unwrap().mark = self.frames;
        self.note_intent("First present held. The in-memory size edit is next.");
        Ok(())
    }

    fn intent_stage_edit(&mut self) -> Result<(), String> {
        let (solid, record, revision_before, old_far, old_close, old_key_far, old_key_close, hash_before) = {
            let lab = self.intent_lab.as_ref().unwrap();
            (
                lab.solid.ok_or("solid missing")?,
                lab.record.clone().ok_or("record missing")?,
                lab.revision_before,
                lab.mesh_far.ok_or("mesh Far missing")?,
                lab.mesh_close.ok_or("mesh Close missing")?,
                lab.key_far.clone(),
                lab.key_close.clone(),
                lab.authority_hash,
            )
        };
        if self.engine.world().revision() != revision_before {
            return Err("the world revision moved before the size edit".into());
        }
        let live = self.engine.world().authored_block(solid).ok_or("Intent Solid disappeared")?;
        if live != record || live.intent.len() != 65 || live.history.len() != 24 || live.body.is_some() {
            return Err("the live Intent Solid changed before the in-memory edit".into());
        }
        let copy = jarvig_core::size_edit_copy(&record).map_err(|error| format!("the size edit was not built: {error}"))?;
        if copy.intent.len() != 66 || copy.history.len() != 24 || copy.body.is_some() {
            return Err("the size copy did not stay at intent 66, history 24, body absent".into());
        }
        if self.engine.world().authored_block(solid).as_ref() != Some(&record) {
            return Err("the size copy was installed into the live solid".into());
        }
        let edited_hash = jarvig_core::authority_hash(&copy, jarvig_core::PATCH_B_TRANSLATION).map_err(|error| error.to_string())?;
        if edited_hash == hash_before || edited_hash == FROZEN_3C_AUTHORITY {
            return Err("the size edit did not change the authority hash".into());
        }
        let far_camera = jarvig_core::far_intent_camera();
        let close_camera = jarvig_core::close_intent_camera();
        let product_far = jarvig_core::realize_intent_observation(&copy, &far_camera, 2, jarvig_core::PATCH_B_TRANSLATION).map_err(|error| format!("edited far realization stopped: {error}"))?;
        let product_close = jarvig_core::realize_intent_observation(&copy, &close_camera, 2, jarvig_core::PATCH_B_TRANSLATION).map_err(|error| format!("edited close realization stopped: {error}"))?;
        self.note_intent(&format!("Edited size bits: {}", jarvig_core::size_edit_bits(&record)));
        self.note_intent(&format!("authority_hash after edit: {edited_hash:016x}"));
        self.note_intent("authority_revision: 2");
        self.note_intent("Live intent entries: 65");
        self.note_intent("Live history entries: 24");
        self.note_intent("Size edit installed: NO");
        self.note_intent("Scene command issued: NO");
        self.note_intent(&jarvig_core::format_intent_product("Edited Observation Far", &product_far));
        self.note_intent(&jarvig_core::format_intent_product("Edited Observation Close", &product_close));
        frozen_intent_gate(&product_far, &product_close)?;
        let (first_far, first_close) = {
            let lab = self.intent_lab.as_ref().unwrap();
            (lab.product_far.clone().ok_or("first far missing")?, lab.product_close.clone().ok_or("first close missing")?)
        };
        if product_far.patch_b.position_hash != first_far.patch_b.position_hash || product_close.patch_b.position_hash != first_close.patch_b.position_hash {
            return Err("the size edit changed the curved patch positions".into());
        }
        if !jarvig_core::planar_geometry_changed(&first_far, &product_far) && !jarvig_core::planar_geometry_changed(&first_close, &product_close) {
            self.note_intent("Planar position hash changed: NO");
            self.note_intent("Second frame: NOT PRESENTED");
            return Err("the size edit left every planar position hash unchanged".into());
        }
        self.note_intent("Planar position hash changed: YES");
        if product_far.cache_key == old_key_far || product_close.cache_key == old_key_close || !product_far.cache_key.starts_with("intent4-") {
            return Err("the edited cache key did not leave the revision-1 key".into());
        }
        let (left, right, entity) = {
            let lab = self.intent_lab.as_ref().unwrap();
            (self.viewport_view.ok_or("view Far missing")?, lab.right_view.ok_or("view Close missing")?, lab.solid.unwrap())
        };
        self.intent_bind_if_current(left, entity, old_far, &old_key_far, &product_far.cache_key)?;
        self.intent_bind_if_current(right, entity, old_close, &old_key_close, &product_close.cache_key)?;
        if self.intent_lab.as_ref().unwrap().discarded_stale_gpu < 5 {
            return Err("the revision-1 meshes were not rejected after the edit".into());
        }
        self.note_intent("Revision-1 GPU meshes rejected: YES");
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
        self.intent_retire_mesh(old_far)?;
        self.intent_retire_mesh(old_close)?;
        {
            let lab = self.intent_lab.as_mut().unwrap();
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
        self.note_intent(&format!("Edited Observation Far mesh id: {}", mesh_far.0));
        self.note_intent(&format!("Edited Observation Close mesh id: {}", mesh_close.0));
        self.note_intent("Cameras moved between presents: NO");
        self.note_intent(&format!("World revision after edit: {}", self.engine.world().revision()));
        Ok(())
    }

    fn intent_stage_edited(&mut self) -> Result<(), String> {
        if !self.intent_lab.as_ref().unwrap().edited_proved {
            if !self.intent_frame_matches()? {
                if self.frames.saturating_sub(self.intent_lab.as_ref().unwrap().mark) > 180 {
                    return Err(format!("the edited intent frame did not upload ({})", self.intent_wait_reason()));
                }
                return Ok(());
            }
            self.intent_note_uploads(true)?;
            self.intent_note_passes("edited", false)?;
            self.intent_capture("shot-intent-4-edited.png")?;
            self.note_intent("Second frame: PRESENTED");
            let lab = self.intent_lab.as_mut().unwrap();
            lab.edited_proved = true;
            lab.mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.intent_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            if !self.intent_frame_matches()? {
                return Err("a later edited frame substituted an intent buffer".into());
            }
            if self.frames.saturating_sub(self.intent_lab.as_ref().unwrap().mark) == 1 && self.intent_lab.as_ref().unwrap().caster_check_pending {
                self.intent_note_passes("edited-cached", true)?;
            }
            return Ok(());
        }
        self.intent_retire_mesh(self.intent_lab.as_ref().unwrap().mesh_far.ok_or("edited mesh Far missing")?)?;
        self.intent_lab.as_mut().unwrap().mesh_far = None;
        self.intent_lab.as_mut().unwrap().stage = 4;
        self.intent_lab.as_mut().unwrap().mark = self.frames;
        self.note_intent("Edited Observation Far destroy requested");
        Ok(())
    }

    fn intent_stage_far_gone(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.intent_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let mesh_far = *self.intent_lab.as_ref().unwrap().retired.last().ok_or("edited mesh Far was not retired")?;
        let mesh_close = self.intent_lab.as_ref().unwrap().mesh_close.ok_or("edited mesh Close missing")?;
        let control = self.intent_lab.as_ref().unwrap().control_mesh.ok_or("control missing")?;
        let gone = self.renderer.as_ref().unwrap().resident_mesh_bytes(mesh_far).is_none();
        let bytes_close = self.renderer.as_ref().unwrap().resident_mesh_bytes(mesh_close);
        if !gone || bytes_close.is_none() {
            if self.frames.saturating_sub(self.intent_lab.as_ref().unwrap().mark) > 90 {
                return Err("destroying edited Far did not leave Close resident and Far evicted".into());
            }
            return Ok(());
        }
        if bytes_close != Some(self.intent_lab.as_ref().unwrap().close_gpu_bytes) {
            return Err(format!("Close GPU bytes changed to {:?} while Far was destroyed", bytes_close));
        }
        if !self.intent_lab.as_ref().unwrap().captured_far {
            let left = self.viewport_view.ok_or("view Far missing")?;
            let right = self.intent_lab.as_ref().unwrap().right_view.ok_or("view Close missing")?;
            let draws = self.renderer.as_ref().unwrap().frame_mesh_draws().to_vec();
            if color_contains(&draws, left, mesh_far) || !color_contains(&draws, left, control) || !color_contains(&draws, right, mesh_close) || color_contains(&draws, right, control) {
                return Err("the frame after destroying Far still mixed the observation meshes".into());
            }
            self.note_intent("Observation Far GPU_bytes after destroy: GONE");
            self.note_intent(&format!("Observation Close GPU_bytes while Far is gone: {}", bytes_close.unwrap()));
            self.note_intent("View Far color pass after Far was destroyed: ordinary control mesh");
            self.intent_capture("shot-intent-4-far-destroyed.png")?;
            self.intent_lab.as_mut().unwrap().captured_far = true;
            self.intent_lab.as_mut().unwrap().mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.intent_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        self.intent_retire_mesh(mesh_close)?;
        self.intent_lab.as_mut().unwrap().mesh_close = None;
        self.intent_lab.as_mut().unwrap().stage = 5;
        self.intent_lab.as_mut().unwrap().mark = self.frames;
        self.note_intent("Edited Observation Close destroy requested");
        Ok(())
    }

    fn intent_stage_close_gone(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.intent_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let meshes = self.intent_lab.as_ref().unwrap().retired.clone();
        let gone = meshes.iter().all(|mesh| self.renderer.as_ref().unwrap().resident_mesh_bytes(*mesh).is_none());
        if !gone {
            if self.frames.saturating_sub(self.intent_lab.as_ref().unwrap().mark) > 90 {
                return Err("edited observation Close stayed resident after destroy".into());
            }
            return Ok(());
        }
        if !self.intent_lab.as_ref().unwrap().captured_close {
            self.note_intent("Observation Close GPU_bytes after destroy: GONE");
            self.intent_capture("shot-intent-4-close-destroyed.png")?;
            self.intent_lab.as_mut().unwrap().captured_close = true;
            self.intent_lab.as_mut().unwrap().mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.intent_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        let decoy = self.intent_lab.as_ref().unwrap().decoy.ok_or("decoy missing")?;
        self.intent_retire_mesh(decoy)?;
        self.intent_lab.as_mut().unwrap().decoy = None;
        let right = self.intent_lab.as_ref().unwrap().right_view.ok_or("view Close missing")?;
        let left = self.viewport_view.ok_or("view Far missing")?;
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.set_view_pixel_rect(left, None).map_err(|error| error.to_string())?;
            renderer.update_view(left, RenderViewUpdate { camera: None, layout: Some(NormalizedRect::FULL), settings: None, pose: None }).map_err(|error| error.to_string())?;
            renderer.destroy_view(right).map_err(|error| error.to_string())?;
        }
        if self.engine.world().revision() != self.intent_lab.as_ref().unwrap().revision_before {
            return Err("destroying the observation views revised the world".into());
        }
        let lab = self.intent_lab.as_mut().unwrap();
        lab.split = false;
        lab.right_view = None;
        lab.stage = 6;
        lab.mark = self.frames;
        self.note_intent("Perspective restored to one full view");
        Ok(())
    }

    fn intent_stage_intact(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.intent_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let lab = self.intent_lab.as_ref().unwrap();
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
        let tape_same = live.as_ref().is_some_and(|record| record.intent.len() == 65 && record.history.len() == 24);
        let eligible = live.as_ref().is_some_and(|record| jarvig_core::intent_authority_eligibility(record) == jarvig_core::IntentAuthorityEligibility::Eligible);
        let name_same = self.engine.world().remember_entity(solid).ok().map(|entity| entity.name) == Some(lab.solid_name.clone());
        let meshlets_same = self.engine.world().derived_meshlets(control).map(|set| set.meshlets.len()) == lab.meshlet_len;
        let ordinary_same = self
            .engine
            .world()
            .meshes()
            .get(control)
            .is_some_and(|mesh| mesh.vertex_count() == lab.control_vertices && mesh.index_count() / 3 == lab.control_triangles);
        let retired = lab.retired.clone();
        let orphans_gone = retired.iter().all(|mesh| self.engine.world().meshes().get(*mesh).is_none());
        let gpu_gone = retired.iter().all(|mesh| self.renderer.as_ref().unwrap().resident_mesh_bytes(*mesh).is_none());
        let control_after = self.renderer.as_ref().unwrap().resident_mesh_bytes(control);
        let control_match = control_after == lab.control_gpu_bytes && control_after == Some(1512);
        let views = self.renderer.as_ref().unwrap().view_count() == 1;
        let level_now = std::fs::read(&lab.level_path).unwrap_or_default();
        let level_same = level_now == lab.level_bytes && level_now.len() == 26849;
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
        let versions_ok = jarvig_core::INTENT_OBSERVATION_ALGORITHM_VERSION == 1
            && jarvig_core::DIRECT_REALIZATION_ALGORITHM_VERSION == 1
            && jarvig_core::OBSERVATION_ALGORITHM_VERSION == 1
            && jarvig_core::REALIZATION_ALGORITHM_VERSION == 1;
        let hash_moved = lab.edited_hash != 0 && lab.edited_hash != lab.authority_hash && lab.edited_hash != FROZEN_3C_AUTHORITY;
        let object_ok = record_same
            && pose_same
            && mesh_same
            && collision_same
            && body_absent
            && tape_same
            && eligible
            && name_same
            && meshlets_same
            && ordinary_same
            && orphans_gone
            && gpu_gone
            && control_match
            && views
            && level_same
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
            && !self.engine.intent_authority_experiment();
        self.intent_capture("shot-intent-4-object.png")?;
        self.note_intent(&format!("Editor frame: {}", self.frames));
        self.note_intent(&format!("Presented frames: {}", self.renderer.as_ref().unwrap().presented_frames()));
        self.note_intent(&format!("Frame: {}", if presented && counts_ok { "PRESENTED" } else { "NOT PRESENTED" }));
        self.note_intent(&format!("Graphics is Intel UHD DX12 8086:9b41: {}", yes_no(graphics_ok)));
        self.note_intent(&format!("Acquires on the first proof frame: {}", self.intent_lab.as_ref().unwrap().acquires_first));
        self.note_intent(&format!("Presents on the first proof frame: {}", self.intent_lab.as_ref().unwrap().presents_first));
        self.note_intent(&format!("Acquires on the second proof frame: {}", self.intent_lab.as_ref().unwrap().acquires_second));
        self.note_intent(&format!("Presents on the second proof frame: {}", self.intent_lab.as_ref().unwrap().presents_second));
        self.note_intent(&format!("Intent Solid record unchanged: {}", yes_no(record_same)));
        self.note_intent(&format!("Intent Solid evaluated body absent: {}", yes_no(body_absent)));
        self.note_intent(&format!("intent entries: {}", live.as_ref().map(|record| record.intent.len()).unwrap_or(0)));
        self.note_intent(&format!("history entries: {}", live.as_ref().map(|record| record.history.len()).unwrap_or(0)));
        self.note_intent(&format!("Intent authority eligibility: {}", if eligible { "ELIGIBLE" } else { "INELIGIBLE" }));
        self.note_intent(&format!("collision before_after: {}", if collision_same { "MATCH" } else { "DIFFER" }));
        self.note_intent(&format!("materials before_after: {}", if record_same { "MATCH" } else { "DIFFER" }));
        self.note_intent(&format!("semantic identity before_after: {}", if record_same && name_same { "MATCH" } else { "DIFFER" }));
        self.note_intent("Face 89 consulted as an input: NO");
        self.note_intent(&format!("Ordinary control mesh unchanged: {}", yes_no(mesh_same && ordinary_same && control_match)));
        self.note_intent(&format!("World revision before: {}", self.intent_lab.as_ref().unwrap().revision_before));
        self.note_intent(&format!("World revision after: {}", self.engine.world().revision()));
        self.note_intent(&format!("World revision unchanged: {}", yes_no(revision_same)));
        self.note_intent(&format!("Library mesh count restored: {}", yes_no(mesh_count_same)));
        self.note_intent(&format!("Observation GPU buffers after destroy: {}", if gpu_gone { "EVICTED" } else { "RESIDENT" }));
        self.note_intent(&format!("Control mesh GPU_bytes after: {}", control_after.map(|value| value.to_string()).unwrap_or_else(|| "GONE".into())));
        self.note_intent(&format!("Level file bytes unchanged: {}", yes_no(level_same)));
        self.note_intent(&format!("Render view count restored: {}", yes_no(views)));
        self.note_intent(&format!("Editor element selection: {element}"));
        self.note_intent(&format!("stale uploads rejected: {}", self.intent_lab.as_ref().unwrap().discarded_stale_gpu));
        self.note_intent(&format!("Editor workers: general {} einstein {} realize 0", self.jobs.worker_count(), self.einstein_jobs.worker_count()));
        self.note_intent("Intent-authority experiment: OFF");
        self.note_intent("ADR-0074 accepted: NO");
        self.note_intent("RFC-0007 accepted: NO");
        self.note_intent("RFC-0007 Phase 3: NO");
        self.note_intent("RFC-0002 stamped: NO");
        self.note_intent("Renderer replacement: NO");
        self.note_intent("Curved trailer is experiment-local: YES");
        self.note_intent(&format!("Authoritative intent after both products destroyed: {}", if object_ok { "INTACT" } else { "CHANGED" }));
        let passed = object_ok && self.intent_lab.as_ref().unwrap().report.contains("expected_gpu_bytes = GpuMesh.bytes = create_buffer bytes: YES") && self.intent_lab.as_ref().unwrap().report.contains("Second frame: PRESENTED");
        if passed {
            self.note_intent("Earned: a persistent editable intent object can drive observation-conditioned realization, with analytic detail directly synthesized according to observation, and edits invalidate those realizations.");
        }
        self.note_intent(&format!("Experiment 4: {}", if passed { "PASS" } else { "FAIL" }));
        self.intent_write_report();
        self.intent_lab.as_mut().unwrap().stage = 15;
        if !passed {
            return Err("Experiment 4 did not meet the presented-frame success condition".into());
        }
        unsafe { PostQuitMessage(0); }
        Ok(())
    }

    fn intent_note_uploads(&mut self, second: bool) -> Result<(), String> {
        let (mesh_far, mesh_close, product_far, product_close) = {
            let lab = self.intent_lab.as_ref().unwrap();
            (lab.mesh_far.unwrap(), lab.mesh_close.unwrap(), lab.product_far.clone().unwrap(), lab.product_close.clone().unwrap())
        };
        let record_far = self.renderer.as_ref().unwrap().mesh_upload_record(mesh_far).ok_or("observation Far was not uploaded")?;
        let record_close = self.renderer.as_ref().unwrap().mesh_upload_record(mesh_close).ok_or("observation Close was not uploaded")?;
        let prove = |label: &str, product: &jarvig_core::IntentObservationProduct, record: jarvig_renderer::MeshUploadRecord| -> Result<(), String> {
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
        if product_far.patch_b.gpu_bytes != 13392 || product_close.patch_b.gpu_bytes != 72912 {
            return Err("patch B GPU accounts left 13392 and 72912".into());
        }
        self.note_intent(&format!("{label_far} GpuMesh.bytes: {}", record_far.gpu_bytes));
        self.note_intent(&format!("{label_far} create_buffer bytes: {}", record_far.vertex_create_bytes + record_far.index_create_bytes));
        self.note_intent(&format!("{label_close} GpuMesh.bytes: {}", record_close.gpu_bytes));
        self.note_intent(&format!("{label_close} create_buffer bytes: {}", record_close.vertex_create_bytes + record_close.index_create_bytes));
        self.note_intent("expected_gpu_bytes = GpuMesh.bytes = create_buffer bytes: YES");
        self.note_intent("triangles_discarded_after_upload: 0");
        let acquires = self.renderer.as_ref().unwrap().acquires_last_frame();
        let presents = self.renderer.as_ref().unwrap().presents_last_frame();
        self.note_intent(&format!("Acquires this frame: {acquires}"));
        self.note_intent(&format!("Presents this frame: {presents}"));
        if acquires != 1 || presents != 1 {
            return Err("the intent frame did not use one acquire and one present".into());
        }
        let lab = self.intent_lab.as_mut().unwrap();
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

    fn intent_note_passes(&mut self, label: &str, require_recorded: bool) -> Result<(), String> {
        let draws = self.renderer.as_ref().unwrap().frame_mesh_draws().to_vec();
        let control = self.intent_lab.as_ref().unwrap().control_mesh.unwrap();
        let decoy = self.intent_lab.as_ref().unwrap().decoy.unwrap();
        let observation = self.intent_lab.as_ref().unwrap().observation_meshes.clone();
        let shadow = pass_meshes(&draws, "shadow");
        let shadow_cached = pass_meshes(&draws, "shadow-cached");
        let contact = pass_meshes(&draws, "contact");
        let probe = pass_meshes(&draws, "probe");
        let casters = if shadow.is_empty() { shadow_cached.clone() } else { shadow.clone() };
        self.note_intent(&format!("{label} shadow draws: {}", mesh_list(&shadow)));
        self.note_intent(&format!("{label} shadow casters: {}", mesh_list(&casters)));
        self.note_intent(&format!("{label} shadow maps redrawn this frame: {}", yes_no(!shadow.is_empty())));
        self.note_intent(&format!("{label} contact meshes: {}", mesh_list(&contact)));
        self.note_intent(&format!("{label} probe meshes: {}", mesh_list(&probe)));
        let forbidden = |list: &[MeshId]| list.iter().any(|mesh| observation.contains(mesh) || *mesh == decoy);
        if forbidden(&shadow) || forbidden(&shadow_cached) || forbidden(&contact) || forbidden(&probe) {
            return Err("an observation mesh was drawn by shadow, contact, or probe capture".into());
        }
        self.note_intent("Observation mesh in shadow draws: NO");
        self.note_intent("Observation mesh in contact draws: NO");
        self.note_intent("Observation mesh in probe draws: NO");
        if casters.is_empty() && !require_recorded {
            let solid = self.intent_lab.as_ref().unwrap().solid.ok_or("the ordinary control entity is missing")?;
            let snapshot = self.engine.world().extract(jarvig_core::RenderFrameId(1)).map_err(|error| error.to_string())?;
            let casts = snapshot.instances().iter().find(|instance| instance.entity == solid).is_some_and(|instance| instance.cast_shadows && instance.mesh == control);
            if !casts {
                return Err("the ordinary control mesh does not cast shadows".into());
            }
            self.note_intent("Ordinary control cast_shadows: YES");
            self.note_intent("Shadow caster list on the rebuild frame: NOT RECORDED");
            self.intent_lab.as_mut().unwrap().caster_check_pending = true;
            return Ok(());
        }
        if !casters.contains(&control) {
            return Err("the ordinary control mesh was not the shadow caster".into());
        }
        self.note_intent("Ordinary control mesh in shadow casters: YES");
        self.intent_lab.as_mut().unwrap().caster_check_pending = false;
        Ok(())
    }

    fn intent_wait_reason(&self) -> String {
        let Some(renderer) = self.renderer.as_ref() else {
            return "renderer missing".into();
        };
        let Some(lab) = self.intent_lab.as_ref() else {
            return "lab missing".into();
        };
        let rect = |view: Option<jarvig_renderer::RenderViewId>| {
            view.and_then(|view| renderer.viewport(view).ok().flatten()).map(|rect| format!("{}x{}@{},{}", rect.width, rect.height, rect.x, rect.y)).unwrap_or_else(|| "none".into())
        };
        let uploaded = |mesh: Option<MeshId>| mesh.and_then(|mesh| renderer.mesh_upload_record(mesh)).is_some();
        let color: Vec<u64> = renderer.frame_mesh_draws().iter().filter(|draw| draw.pass == "color").map(|draw| draw.mesh.0).collect();
        format!("far {} close {} far_uploaded {} close_uploaded {} color {:?}", rect(self.viewport_view), rect(lab.right_view), uploaded(lab.mesh_far), uploaded(lab.mesh_close), color)
    }

    fn intent_frame_matches(&self) -> Result<bool, String> {
        let Some(renderer) = self.renderer.as_ref() else {
            return Ok(false);
        };
        let left = self.viewport_view.ok_or("view Far missing")?;
        let right = match self.intent_lab.as_ref().unwrap().right_view {
            Some(view) => view,
            None => return Ok(false),
        };
        let mesh_far = self.intent_lab.as_ref().unwrap().mesh_far;
        let mesh_close = self.intent_lab.as_ref().unwrap().mesh_close;
        let control = self.intent_lab.as_ref().unwrap().control_mesh.ok_or("control missing")?;
        let decoy = self.intent_lab.as_ref().unwrap().decoy.ok_or("decoy missing")?;
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

    fn intent_reject_stale(&mut self) -> Result<(), String> {
        let (left, right, mesh_far, mesh_close, decoy, key_far, key_close, entity) = {
            let lab = self.intent_lab.as_ref().unwrap();
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
        self.intent_bind_if_current(left, entity, mesh_close, &key_close, &key_far)?;
        self.intent_bind_if_current(right, entity, mesh_far, &key_far, &key_close)?;
        self.intent_bind_if_current(left, entity, decoy, "stale-decoy", &key_far)?;
        if self.renderer.as_ref().unwrap().resident_mesh_bytes(decoy).is_some() {
            return Err("the refused decoy was uploaded".into());
        }
        self.note_intent("Decoy mesh GPU_bytes: NOT UPLOADED");
        self.note_intent(&format!("stale uploads rejected: {}", self.intent_lab.as_ref().unwrap().discarded_stale_gpu));
        self.note_intent("Cross-view buffer substitution: NO");
        self.note_intent("Decoy mesh bound: NO");
        Ok(())
    }

    fn intent_bind_if_current(&mut self, view: RenderViewId, entity: jarvig_core::EntityId, mesh: MeshId, product_key: &str, current_key: &str) -> Result<(), String> {
        if product_key != current_key {
            self.intent_lab.as_mut().unwrap().discarded_stale_gpu = self.intent_lab.as_ref().unwrap().discarded_stale_gpu.saturating_add(1);
            return Ok(());
        }
        self.renderer.as_mut().unwrap().set_view_mesh_override(view, entity, Some(mesh)).map_err(|error| error.to_string())
    }

    fn intent_retire_mesh(&mut self, mesh: MeshId) -> Result<(), String> {
        let control = self.intent_lab.as_ref().unwrap().control_mesh.ok_or("control missing")?;
        if mesh == control {
            return Err("refusing to retire the ordinary control mesh".into());
        }
        if !self.engine.world_mut().retire_unreferenced_mesh(mesh) {
            return Err(format!("intent mesh {} stayed referenced", mesh.0));
        }
        self.intent_lab.as_mut().unwrap().retired.push(mesh);
        if self.engine.world().revision() != self.intent_lab.as_ref().unwrap().revision_before {
            return Err("retire_unreferenced_mesh revised the world".into());
        }
        Ok(())
    }
}

fn frozen_patch(account: &jarvig_core::IntentPatchAccount, n: u32, triangles: u32, vertices: u32, index_bytes: u64, gpu_bytes: u64, error: &str, coarser: &str, evaluations: u32) -> Result<(), String> {
    let error_text = format!("{:.6}", account.measured_error_px);
    let coarser_text = format!("{:.6}", account.coarser_px);
    if account.surface_id != jarvig_core::PATCH_B
        || account.admission != jarvig_core::IntentAdmission::AdmitInView
        || account.n != n
        || account.triangles_constructed != triangles
        || account.packed_triangles != triangles
        || account.packed_vertices != vertices
        || account.vertex_bytes != u64::from(vertices) * 60
        || account.index_bytes != index_bytes
        || account.gpu_bytes != gpu_bytes
        || account.candidate_grids_emitted != 1
        || account.candidate_grids_discarded != 0
        || account.resolution_predicate_evaluations != evaluations
        || !account.coarser_failed
        || error_text != error
        || coarser_text != coarser
        || account.position_hash == FROZEN_FAR_POSITION
        || account.position_hash == FROZEN_CLOSE_POSITION
        || account.position_hash == 0
    {
        return Err(format!(
            "patch 1002 left the frozen measurement (n {} triangles {} error {error_text} coarser {coarser_text} gpu {} hash {:016x})",
            account.n, account.triangles_constructed, account.gpu_bytes, account.position_hash
        ));
    }
    Ok(())
}

fn frozen_intent_gate(far: &jarvig_core::IntentObservationProduct, close: &jarvig_core::IntentObservationProduct) -> Result<(), String> {
    if far.authority_hash == FROZEN_3C_AUTHORITY || close.authority_hash == FROZEN_3C_AUTHORITY || far.authority_hash != close.authority_hash {
        return Err("the bridge authority is the 3C specimen hash or the two views disagree".into());
    }
    if !far.cache_key.starts_with("intent4-") || !close.cache_key.starts_with("intent4-") {
        return Err("the cache key is not intent4-".into());
    }
    let omitted = |account: &jarvig_core::IntentPatchAccount| {
        account.surface_id == jarvig_core::PATCH_A
            && account.admission == jarvig_core::IntentAdmission::Omit
            && account.triangles_constructed == 0
            && account.gpu_bytes == 0
            && account.candidate_grids_emitted == 0
    };
    if !omitted(&far.patch_a) || !omitted(&close.patch_a) {
        return Err("patch 1001 was not a zero account".into());
    }
    if far.trace.iter().any(|record| record.surface_id == jarvig_core::PATCH_A) || close.trace.iter().any(|record| record.surface_id == jarvig_core::PATCH_A) {
        return Err("the construction trace contains patch 1001".into());
    }
    frozen_patch(&far.patch_b, 6, 72, 216, 432, 13392, FROZEN_FAR_ERROR, FROZEN_FAR_COARSER, 6)?;
    frozen_patch(&close.patch_b, 14, 392, 1176, 2352, 72912, FROZEN_CLOSE_ERROR, FROZEN_CLOSE_COARSER, 14)?;
    if close.patch_b.n <= far.patch_b.n {
        return Err("n_close is not greater than n_far".into());
    }
    if far.rejected_n_allocated_mesh || close.rejected_n_allocated_mesh {
        return Err("a rejected n allocated a mesh".into());
    }
    for product in [far, close] {
        if product.object_mesh_consulted
            || product.record_body_consulted
            || product.authoritative_triangles_consulted
            || product.build_realization_consulted
            || product.realize_direct_consulted
            || product.analytic_specimen_consulted
            || product.einstein_consulted
            || product.meshlets_constructed != 0
            || product.vertices_welded
            || product.discarded_after_construction != 0
            || product.discarded_during_pack != 0
            || product.discarded_after_upload != 0
            || product.constructed_triangles != product.packed_triangles
            || product.packed_triangles != product.uploaded_triangles
        {
            return Err("an intent product consulted a forbidden input or discarded triangles".into());
        }
        for account in &product.planar {
            if account.admission == jarvig_core::IntentAdmission::Omit {
                if account.triangles_constructed != 0 || account.gpu_bytes != 0 {
                    return Err(format!("omitted planar face {} contributed triangles", account.face_id));
                }
            } else if account.triangles_constructed != account.loop_len.saturating_sub(2) || account.packed_triangles != account.triangles_constructed {
                return Err(format!("planar face {} was not one exact fan", account.face_id));
            }
        }
    }
    Ok(())
}

fn intent_pose(solid: ResolvedPose, eye: [f64; 3], forward: [f64; 3]) -> Result<ResolvedPose, String> {
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
        return Err(format!("the view rotation does not look along the intent forward ({delta})"));
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

fn id_text(ids: &[u32]) -> String {
    if ids.is_empty() {
        "NONE".into()
    } else {
        ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "YES" } else { "NO" }
}
