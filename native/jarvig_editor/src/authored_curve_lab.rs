//! Experiment 7. One persisted semantic round, synthesized per observation.
//!
//! Far is destroyed before Close. The radius edit happens after both are gone.
//! This lab does not enable intent authority and does not edit the earlier labs.

use jarvig_core::{MeshId, Quat, ResolvedPose, Vec3};
use jarvig_renderer::{NormalizedRect, PixelRect, RenderViewDesc, RenderViewId, RenderViewSettings, RenderViewUpdate};
use windows_sys::Win32::Foundation::{COLORREF, GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetStockObject, InvalidateRect, SelectObject, SetBkMode, SetTextColor, UpdateWindow, DEFAULT_GUI_FONT, DT_LEFT,
    DT_NOPREFIX, DT_TOP, PAINTSTRUCT, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClassInfoW, GetClientRect, GetWindowRect, GetWindowTextW, PostQuitMessage, RegisterClassW, SetWindowPos, SetWindowTextW, CS_HREDRAW,
    CS_VREDRAW, HTTRANSPARENT, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NCHITTEST, WM_PAINT, WM_PRINTCLIENT, WNDCLASSW,
    WS_CLIPSIBLINGS, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
};

use crate::dock::{WorkspaceCommand, CONTENT, INSPECTOR, OUTLINER, OUTPUT, PERSPECTIVE};

use super::Editor;

const HOLD_SHOT: u32 = 24;
const VIEW_FAR_WIDTH: u32 = 520;
const VIEW_CLOSE_WIDTH: u32 = 980;
const VIEW_HEIGHT: u32 = 700;
const REQUIRED_WIDTH: u32 = VIEW_FAR_WIDTH + VIEW_CLOSE_WIDTH;
const CURVE_SOLID_UUID: &str = "e7e7e7e7-e7e7-47e7-87e7-e7e7e7e7e7ea";

pub(super) struct CurveLab {
    stage: u8,
    arranged: bool,
    split: bool,
    grow: u8,
    mark: u32,
    proved: bool,
    edited_proved: bool,
    captured_far_gone: bool,
    captured_close_gone: bool,
    right_view: Option<RenderViewId>,
    solid: Option<jarvig_core::EntityId>,
    legacy: Option<jarvig_core::EntityId>,
    control_mesh: Option<MeshId>,
    legacy_mesh: Option<MeshId>,
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
    product_far: Option<jarvig_core::RoundProduct>,
    product_close: Option<jarvig_core::RoundProduct>,
    authority_hash: u64,
    edited_hash: u64,
    fov: f64,
    token: String,
    feature_id: String,
    edge_id: u32,
    control_vertices: u32,
    control_triangles: u32,
    control_gpu_bytes: Option<u64>,
    gpu_bytes_close: u64,
    revision_before: u64,
    acquires_first: u32,
    presents_first: u32,
    acquires_second: u32,
    presents_second: u32,
    panel: (u32, u32),
    discarded_stale_gpu: u32,
    discarded_before_edit: u32,
    caster_check_pending: bool,
    level_path: String,
    report: String,
    phase_name: String,
    hud: HWND,
    corner_far: Option<MeshId>,
    corner_close: Option<MeshId>,
    wire_far: Option<MeshId>,
    wire_close: Option<MeshId>,
    divisions_far: u32,
    divisions_close: u32,
    radius_m: f64,
    hud_text: String,
}

impl CurveLab {
    pub(super) fn reference_suppressed(&self) -> bool {
        self.split
    }

    pub(super) fn status_text(&self) -> Option<String> {
        if self.hud_text.is_empty() { None } else { Some(self.hud_text.replace('\n', " | ")) }
    }

    pub(super) fn new() -> Self {
        Self {
            stage: 0,
            arranged: false,
            split: false,
            grow: 0,
            mark: 0,
            proved: false,
            edited_proved: false,
            captured_far_gone: false,
            captured_close_gone: false,
            right_view: None,
            solid: None,
            legacy: None,
            control_mesh: None,
            legacy_mesh: None,
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
            fov: 0.0,
            token: String::new(),
            feature_id: String::new(),
            edge_id: 0,
            control_vertices: 0,
            control_triangles: 0,
            control_gpu_bytes: None,
            gpu_bytes_close: 0,
            revision_before: 0,
            acquires_first: 0,
            presents_first: 0,
            acquires_second: 0,
            presents_second: 0,
            panel: (0, 0),
            discarded_stale_gpu: 0,
            discarded_before_edit: 0,
            caster_check_pending: false,
            level_path: String::new(),
            report: String::new(),
            phase_name: "author".into(),
            hud: std::ptr::null_mut(),
            corner_far: None,
            corner_close: None,
            wire_far: None,
            wire_close: None,
            divisions_far: 0,
            divisions_close: 0,
            radius_m: 0.0,
            hud_text: String::new(),
        }
    }
}

impl Editor {
    pub(super) fn apply_curve_frame(&mut self) -> Result<(), String> {
        let split = self.curve_lab.as_ref().is_some_and(|lab| lab.split);
        if self.curve_lab.is_none() {
            return Ok(());
        }
        if !split {
            return Ok(());
        }
        let lab = self.curve_lab.as_ref().unwrap();
        let right = lab.right_view.ok_or("Close's view is missing")?;
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

    pub(super) fn step_curve_lab(&mut self) -> Result<(), String> {
        if self.curve_lab.is_none() {
            return Ok(());
        }
        let stage = self.curve_lab.as_ref().unwrap().stage;
        self.place_curve_hud();
        if stage >= 15 {
            return Ok(());
        }
        let phase = self.curve_lab.as_ref().unwrap().phase_name.clone();
        let result = match (phase.as_str(), stage) {
            (_, 0) => self.curve_stage_setup(),
            ("author", 8) => self.curve_stage_sharp(),
            ("author", 9) => self.curve_stage_wire(),
            ("author", 10) => self.curve_stage_author_done(),
            ("stay", 10) => self.curve_stage_release(),
            (_, 1) => self.curve_stage_present(),
            (_, 4) => self.curve_stage_edit(),
            (_, 5) => self.curve_stage_edited(),
            ("reload", 6) => self.curve_stage_reload_done(),
            ("stay", 6) => self.curve_stage_release(),
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.note_curve("Experiment 7: FAIL");
            self.note_curve(&format!("Failure: {error}"));
            self.curve_write_report();
            if let Some(lab) = self.curve_lab.as_mut() {
                lab.stage = 15;
                lab.split = false;
            }
            return Err(error);
        }
        Ok(())
    }

    fn note_curve(&mut self, line: &str) {
        self.append(line);
        println!("{line}");
        if let Some(lab) = self.curve_lab.as_mut() {
            lab.report.push_str(line);
            lab.report.push('\n');
        }
    }

    fn curve_artifact_dir(&self) -> std::path::PathBuf {
        self.project_file.as_ref().and_then(|path| path.parent().map(|parent| parent.to_path_buf())).unwrap_or_else(jarvig_core::curve_proof_dir)
    }

    pub(super) fn curve_write_report(&mut self) {
        let Some(lab) = self.curve_lab.as_ref() else {
            return;
        };
        let mut text = lab.report.clone();
        text.push_str("Authored curve report: REPORT-AUTHORED-7.txt\n");
        let dir = self.curve_artifact_dir();
        match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(dir.join("REPORT-AUTHORED-7.txt"), &text)) {
            Ok(()) => self.note_curve("Authored curve report: REPORT-AUTHORED-7.txt"),
            Err(error) => self.note_curve(&format!("Authored curve report was not written: {error}")),
        }
    }

    fn curve_capture(&mut self, name: &str) -> Result<(), String> {
        unsafe {
            SetWindowPos(self.frame, (-1isize) as HWND, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        self.place_curve_hud();
        let hud = self.curve_lab.as_ref().map(|lab| lab.hud).unwrap_or(std::ptr::null_mut());
        if !hud.is_null() {
            unsafe { SetWindowPos(hud, (-1isize) as HWND, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW); }
        }
        std::thread::sleep(std::time::Duration::from_millis(60));
        let path = self.curve_artifact_dir().join(name);
        let image = super::capture_screen_image(self.frame)?;
        unsafe {
            SetWindowPos(self.frame, (-2isize) as HWND, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        super::write_png(&path.to_string_lossy(), &image)?;
        self.note_curve(&format!("Capture: {name}"));
        Ok(())
    }

    fn curve_stage_setup(&mut self) -> Result<(), String> {
        if self.view_lab.is_some()
            || self.view_realization.is_some()
            || self.observation_lab.is_some()
            || self.direct_lab.is_some()
            || self.intent_lab.is_some()
            || self.analytic_lab.is_some()
            || self.authored_lab.is_some()
            || self.intent_live_rerealize
        {
            return Err("Experiment 7 refuses to run beside another realization lab".into());
        }
        if self.engine.intent_authority_experiment() {
            return Err("Experiment 7 refuses the intent-authority experiment flag".into());
        }
        if let Some(project) = &self.project_file {
            let text = project.display().to_string();
            if text.contains("IntentProof")
                || text.contains("jarvig-intent-proof")
                || text.contains("AnalyticIntent")
                || text.contains("jarvig-analytic-intent")
                || text.contains("jarvig-authored-intent")
            {
                return Err("Experiment 7 opened a frozen experiment project".into());
            }
            if !text.contains("jarvig-authored-curve") && !text.contains("AuthoredCurve") {
                return Err("Experiment 7 was not opened on the authored-curve project".into());
            }
        }
        if self.renderer.is_none() || self.editor_camera.is_none() || self.viewport_view.is_none() || self.viewport_px.1 < 64 {
            if self.frames >= 400 {
                return Err("the perspective viewport was not ready".into());
            }
            return Ok(());
        }
        if !self.curve_lab.as_ref().unwrap().arranged {
            for panel in [OUTLINER, INSPECTOR, CONTENT, OUTPUT] {
                if self.workspace.is_open(panel) {
                    self.workspace.apply(WorkspaceCommand::ClosePanel(panel)).map_err(|error| error.to_string())?;
                }
            }
            unsafe { SetWindowPos(self.frame, std::ptr::null_mut(), 20, 20, 1800, 1200, SWP_NOZORDER); }
            self.realize();
            let lab = self.curve_lab.as_mut().unwrap();
            lab.arranged = true;
            lab.mark = self.frames;
            return Ok(());
        }
        let (panel_w, panel_h) = self.viewport_px;
        let renderer_size = self.renderer.as_ref().unwrap().configured_size();
        if panel_w < REQUIRED_WIDTH || panel_h < VIEW_HEIGHT || renderer_size.0 < REQUIRED_WIDTH || renderer_size.1 < VIEW_HEIGHT {
            let grow = self.curve_lab.as_ref().unwrap().grow;
            let mark = self.curve_lab.as_ref().unwrap().mark;
            if grow < 2 && self.frames.saturating_sub(mark) > 8 {
                let (width, height) = if grow == 0 { (2200, 1400) } else { (2600, 1600) };
                unsafe { SetWindowPos(self.frame, std::ptr::null_mut(), 20, 20, width, height, SWP_NOZORDER); }
                self.realize();
                let lab = self.curve_lab.as_mut().unwrap();
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
        self.curve_install(control_gpu_bytes, (panel_w, panel_h))?;
        Ok(())
    }

    fn curve_install(&mut self, control_gpu_bytes: u64, panel: (u32, u32)) -> Result<(), String> {
        let level_path = self.level_file.clone().ok_or("the curve level is not open")?;
        let level_text = std::fs::read_to_string(&level_path).map_err(|error| error.to_string())?;
        let parsed = jarvig_core::parse_level(&level_text).map_err(|error| error.to_string())?;
        let record = block_named(&parsed, "Intent Solid")?;
        let legacy = block_named(&parsed, "Legacy Cube")?;
        let versions = parametric_versions(&level_text)?;
        if versions != [3, 1] && versions != [1, 3] {
            return Err(format!("ParametricBlock versions were {versions:?}, not one 3 and one 1"));
        }
        if level_text.contains("analytic-surface") {
            return Err("the curve level contains analytic-surface".into());
        }
        if parsed.format_version != 6 {
            return Err(format!("level format is {}", parsed.format_version));
        }
        let phase = self.curve_phase_name();
        let expect_radius = if phase == "stay" { jarvig_core::CURVE_EDITED_RADIUS_M } else { jarvig_core::CURVE_RADIUS_M };
        let expect_intent = if phase == "stay" { 67 } else { 66 };
        let (token, radius) = active_round(&record)?;
        if radius.to_bits() != expect_radius.to_bits() {
            return Err(format!("parsed radius bits are {:016x}, phase {phase} expects {:016x}", radius.to_bits(), expect_radius.to_bits()));
        }
        let replayed = jarvig_core::replay_round(&record).map_err(|error| error.to_string())?;
        if replayed.token != token || replayed.radius_m.to_bits() != radius.to_bits() {
            return Err("replay did not resolve the saved semantic edge and radius".into());
        }
        if record.size_m != replayed.cache_extent || record.size_m != jarvig_core::class_c_cache_extent(&record).map_err(|error| error.to_string())? {
            return Err("size_m is not the replay cache".into());
        }
        let feature = jarvig_core::feature_id(&token);
        let gathered = {
            let world = self.engine.world();
            let solid = world.entity_outline().iter().find(|item| item.name == "Intent Solid").map(|item| item.uuid).ok_or("Intent Solid missing")?;
            if solid.to_string() != CURVE_SOLID_UUID {
                return Err(format!("Intent Solid is {solid}, not the curve specimen"));
            }
            let legacy_id = world.entity_outline().iter().find(|item| item.name == "Legacy Cube").map(|item| item.uuid).ok_or("Legacy Cube missing")?;
            let live = world.authored_block(solid).ok_or("Intent Solid has no block")?;
            if live.body.is_some() || record.body.is_some() {
                return Err("Intent Solid has a stored body".into());
            }
            if legacy.body.is_none() {
                return Err("Legacy Cube lost its stored body".into());
            }
            if live.intent.len() != expect_intent || record.intent.len() != expect_intent || live.history.len() != 24 || record.history.len() != 24 {
                return Err(format!("Intent Solid is intent {} history {}, phase {phase} expects intent {expect_intent} and history 24", record.intent.len(), record.history.len()));
            }
            let live_round = active_round(&live)?;
            if live_round.0 != token || live_round.1.to_bits() != radius.to_bits() {
                return Err("the live round is not the parsed round".into());
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
            if front.vertical_fov_radians.to_bits() != jarvig_core::CURVE_FOV.to_bits() {
                return Err(format!(
                    "front-camera fov bits {:016x} are not {:016x}",
                    front.vertical_fov_radians.to_bits(),
                    jarvig_core::CURVE_FOV.to_bits()
                ));
            }
            let pose = world.entity_world_pose(solid).map_err(|error| error.to_string())?;
            let scene_x = pose.translation.x - jarvig_core::BOOTSTRAP_ROOT_M;
            if scene_x.abs() > 1.0e-4 || (pose.translation.y - 1.0).abs() > 1.0e-4 || (pose.translation.z + 4.0).abs() > 1.0e-4 {
                return Err(format!("Intent Solid scene translation is ({scene_x}, {}, {}), not (0, 1, -4)", pose.translation.y, pose.translation.z));
            }
            (solid, legacy_id, object_mesh, legacy_mesh, control_vertices, control_triangles, front, world.revision(), diagnostic)
        };
        let (solid, legacy_id, object_mesh, legacy_mesh, control_vertices, control_triangles, front, revision_before, diagnostic) = gathered;
        let fov = front.vertical_fov_radians;
        let far_camera = jarvig_core::presentation_camera(
            &replayed.fillet,
            jarvig_core::CURVE_PRESENT_FAR_M,
            VIEW_FAR_WIDTH as f32,
            VIEW_HEIGHT as f32,
            fov,
            jarvig_core::CURVE_ERROR_PX,
        )
        .map_err(|error| error.to_string())?;
        let close_camera = jarvig_core::presentation_camera(
            &replayed.fillet,
            jarvig_core::CURVE_PRESENT_CLOSE_M,
            VIEW_CLOSE_WIDTH as f32,
            VIEW_HEIGHT as f32,
            fov,
            jarvig_core::CURVE_ERROR_PX,
        )
        .map_err(|error| error.to_string())?;
        if far_camera.viewport_width != VIEW_FAR_WIDTH as f32 || close_camera.viewport_width != VIEW_CLOSE_WIDTH as f32 || far_camera.viewport_height != VIEW_HEIGHT as f32 {
            return Err("the curve cameras were not 520x700 and 980x700".into());
        }
        let product_far = jarvig_core::realize_round_corner(&record, &far_camera, "Far", 1).map_err(|error| format!("Far stopped: {error}"))?;
        let product_close = jarvig_core::realize_round_corner(&record, &close_camera, "Close", 1).map_err(|error| format!("Close stopped: {error}"))?;
        self.curve_require_product("Far", &product_far, &feature, &token, radius)?;
        self.curve_require_product("Close", &product_close, &feature, &token, radius)?;
        if product_close.chosen_arc_divisions <= product_far.chosen_arc_divisions {
            return Err(format!("Close arc divisions {} are not finer than Far {}", product_close.chosen_arc_divisions, product_far.chosen_arc_divisions));
        }
        let mut poisoned = record.clone();
        poisoned.size_m = [9.0, 9.0, 9.0];
        if !jarvig_core::intent_authority_diagnostic(&poisoned).starts_with("Intent Authority: INELIGIBLE") {
            return Err("a poisoned size cache stayed eligible".into());
        }
        let poisoned_far = jarvig_core::realize_round_corner(&poisoned, &far_camera, "Far", 1).map_err(|error| format!("poisoned Far stopped: {error}"))?;
        if poisoned_far.position_hash != product_far.position_hash || poisoned_far.chosen_arc_divisions != product_far.chosen_arc_divisions {
            return Err("replay consulted size_m".into());
        }
        let main_len = std::fs::metadata(jarvig_core::authored_main_level()).map(|meta| meta.len()).unwrap_or(0);
        let authority = jarvig_core::authored_authority_hash(&record).map_err(|error| error.to_string())?;
        self.note_curve("Experiment 7: ON");
        self.note_curve(&format!("Visual phase: {phase}"));
        self.note_curve("Visible corner: trimmed faces plus analytic arc");
        self.note_curve("Stored tessellated round: ABSENT");
        self.note_curve("Experiment 6 planar fixture: FROZEN");
        self.note_curve("Original Experiment 6 FAIL: FROZEN");
        self.note_curve("Intent-authority experiment: OFF");
        self.note_curve("ADR-0074 accepted: NO");
        self.note_curve("Default renderer replaced: NO");
        self.note_curve("analytic-surface whitelisted: NO");
        self.note_curve("Class C planar rule weakened: NO");
        self.note_curve("ParametricBlock version: 3 for the round, 1 for the legacy cube");
        self.note_curve("Level format: 6");
        self.note_curve("Type registry: 9");
        self.note_curve(&diagnostic);
        self.note_curve("body: ABSENT");
        self.note_curve("intent: PRESENT");
        self.note_curve("round: PRESENT");
        self.note_curve(&format!("round semantic edge id: {token}"));
        self.note_curve(&format!("semantic_feature_id: {feature}"));
        self.note_curve(&format!("concrete edge id: {}", replayed.edge_id));
        self.note_curve(&format!("radius: {:.5} m bits {:016x}", radius, radius.to_bits()));
        self.note_curve(&format!("size_m bits: {}", size_bits(record.size_m)));
        self.note_curve(&format!("replay cache bits: {}", size_bits(replayed.cache_extent)));
        self.note_curve(&format!("planar replay bits: {}", size_bits(replayed.planar_extent)));
        self.note_curve("size_m source: replay-derived cache");
        self.note_curve("replay consulted size_m: NO");
        self.note_curve("Intent Authority: ELIGIBLE");
        self.note_curve(&format!("authority_hash: {authority:016x}"));
        self.note_curve("authority_revision: 1");
        self.note_curve(&format!("Intent length: {}", record.intent.len()));
        self.note_curve(&format!("History length: {}", record.history.len()));
        self.note_curve(&format!("Editor workers: general {} einstein {} realize 0", self.jobs.worker_count(), self.einstein_jobs.worker_count()));
        self.note_curve("queue: NOT USED");
        self.note_curve("Research Features Enabled: none");
        self.note_curve(&format!("Graphics adapter: {}", self.graphics_name));
        self.note_curve(&format!("Graphics vendor: {:04x}", self.graphics_vendor));
        self.note_curve(&format!("Graphics device: {:04x}", self.graphics_device));
        self.note_curve(&format!("Graphics api: {}", self.graphics_api));
        self.note_curve(&format!("Editor panel: {}x{}", panel.0, panel.1));
        self.note_curve("Far viewport: 520x700");
        self.note_curve("Close viewport: 980x700");
        self.note_curve(&format!("Requested error px: {:.1}", far_camera.requested_error_px));
        self.note_curve(&format!("Extracted front-camera vertical_fov_radians: {} bits {:016x}", fov, fov.to_bits()));
        self.note_curve(&format!("Far eye_local: [{:.6}, {:.6}, {:.6}]", far_camera.eye[0], far_camera.eye[1], far_camera.eye[2]));
        self.note_curve(&format!("Close eye_local: [{:.6}, {:.6}, {:.6}]", close_camera.eye[0], close_camera.eye[1], close_camera.eye[2]));
        self.note_curve(&format!("Far distance_m: {:.6}", jarvig_core::CURVE_PRESENT_FAR_M));
        self.note_curve(&format!("Close distance_m: {:.6}", jarvig_core::CURVE_PRESENT_CLOSE_M));
        self.note_curve(&format!("Far arc divisions: {}", product_far.chosen_arc_divisions));
        self.note_curve(&format!("Close arc divisions: {}", product_close.chosen_arc_divisions));
        self.note_curve("Subdivision chosen from projected sagitta before triangles: YES");
        self.note_curve("Maximum-resolution round simplified afterward: NO");
        self.note_curve("Stored tessellated round: ABSENT");
        self.note_curve("Ordinary control mesh: 24 vertices, 12 triangles");
        self.note_curve(&format!("Level file: {}", level_path.display()));
        self.note_curve(&format!("Main.jarviglevel bytes: {main_len}"));
        self.note_curve(&format!("World revision before: {revision_before}"));
        if main_len != jarvig_core::AUTHORED_MAIN_LEVEL_BYTES {
            return Err(format!("Main.jarviglevel is {main_len} bytes"));
        }
        let material_bound = {
            let snapshot = self.engine.world().extract(jarvig_core::RenderFrameId(1)).map_err(|error| error.to_string())?;
            snapshot.instances().iter().find(|instance| instance.entity == solid).and_then(|instance| instance.material_for_slot(0)).is_some()
        };
        if !material_bound {
            self.note_curve("Slot-0 material on Intent Solid: UNBOUND");
            return Err("the override would be skipped because slot 0 is unbound".into());
        }
        self.note_curve("Slot-0 material on Intent Solid: BOUND");
        self.note_curve("bind_material called: NO");
        if self.engine.world().revision() != revision_before {
            return Err("the world revision moved before any observation mesh was added".into());
        }
        let sharp = jarvig_core::sharp_corner_mesh(&replayed.fillet).map_err(|error| format!("sharp corner stopped: {error}"))?;
        let wire_far_mesh = jarvig_core::round_wire_mesh(&replayed.fillet, product_far.chosen_arc_divisions).map_err(|error| format!("Far wire stopped: {error}"))?;
        let wire_close_mesh = jarvig_core::round_wire_mesh(&replayed.fillet, product_close.chosen_arc_divisions).map_err(|error| format!("Close wire stopped: {error}"))?;
        let mesh_far = self.engine.world_mut().add_mesh(product_far.mesh.clone());
        let mesh_close = self.engine.world_mut().add_mesh(product_close.mesh.clone());
        let sharp_far = self.engine.world_mut().add_mesh(sharp.clone());
        let sharp_close = self.engine.world_mut().add_mesh(sharp);
        let wire_far = self.engine.world_mut().add_mesh(wire_far_mesh);
        let wire_close = self.engine.world_mut().add_mesh(wire_close_mesh);
        let decoy = self.engine.world_mut().add_mesh(jarvig_core::near_triangle_mesh());
        let show_sharp = phase == "author";
        let display_far = if show_sharp { sharp_far } else { mesh_far };
        let display_close = if show_sharp { sharp_close } else { mesh_close };
        if self.engine.world().revision() != revision_before {
            return Err("add_mesh revised the world".into());
        }
        if mesh_far == object_mesh || mesh_close == object_mesh || mesh_far == mesh_close || mesh_far == legacy_mesh || mesh_close == legacy_mesh {
            return Err("an observation mesh reused the control mesh or the legacy mesh".into());
        }
        if self.engine.world().object_mesh(solid) != Some(object_mesh) {
            return Err("the object mesh changed when the observation meshes were added".into());
        }
        if self.engine.world().derived_meshlets(mesh_far).is_some() || self.engine.world().derived_meshlets(mesh_close).is_some() {
            return Err("an observation mesh built meshlets".into());
        }
        self.note_curve(&format!("{}gpu_mesh_id: {}", jarvig_core::format_round_product(&product_far), mesh_far.0));
        self.note_curve(&format!("{}gpu_mesh_id: {}", jarvig_core::format_round_product(&product_close), mesh_close.0));
        let solid_pose = self.engine.world().entity_world_pose(solid).map_err(|error| error.to_string())?;
        let pose_far = curve_pose(solid_pose, far_camera.eye, far_camera.forward)?;
        let pose_close = curve_pose(solid_pose, close_camera.eye, close_camera.forward)?;
        let camera = jarvig_core::Camera { frame: front.frame, vertical_fov_radians: fov, near_m: far_camera.near_m as f32 };
        let target = self.target.ok_or("viewport target missing")?;
        let left = self.viewport_view.ok_or("the perspective view is missing")?;
        let right = {
            let renderer = self.renderer.as_mut().unwrap();
            renderer
                .create_view(RenderViewDesc { label: "JARVIG.Perspective.Close".into(), target, camera, layout: NormalizedRect::FULL, settings: RenderViewSettings::default() })
                .map_err(|error| error.to_string())?
        };
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.set_view_pixel_rect(left, Some(PixelRect { x: 0, y: 0, width: VIEW_FAR_WIDTH, height: VIEW_HEIGHT })).map_err(|error| error.to_string())?;
            renderer
                .set_view_pixel_rect(right, Some(PixelRect { x: VIEW_FAR_WIDTH, y: 0, width: VIEW_CLOSE_WIDTH, height: VIEW_HEIGHT }))
                .map_err(|error| error.to_string())?;
            renderer.update_view(left, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose_far) }).map_err(|error| error.to_string())?;
            renderer.update_view(right, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose_close) }).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(left, solid, Some(display_far)).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(right, solid, Some(display_close)).map_err(|error| error.to_string())?;
        }
        if self.engine.world().revision() != revision_before {
            return Err("creating the observation views revised the world".into());
        }
        let key_far = curve_key(&product_far);
        let key_close = curve_key(&product_close);
        {
            let lab = self.curve_lab.as_mut().unwrap();
            lab.solid = Some(solid);
            lab.legacy = Some(legacy_id);
            lab.control_mesh = Some(object_mesh);
            lab.legacy_mesh = Some(legacy_mesh);
            lab.mesh_far = Some(display_far);
            lab.mesh_close = Some(display_close);
            lab.corner_far = Some(mesh_far);
            lab.corner_close = Some(mesh_close);
            lab.wire_far = Some(wire_far);
            lab.wire_close = Some(wire_close);
            lab.decoy = Some(decoy);
            lab.observation_meshes.extend([mesh_far, mesh_close, sharp_far, sharp_close, wire_far, wire_close]);
            lab.phase_name = phase.clone();
            lab.divisions_far = product_far.chosen_arc_divisions;
            lab.divisions_close = product_close.chosen_arc_divisions;
            lab.radius_m = radius;
            lab.right_view = Some(right);
            lab.key_far = key_far;
            lab.key_close = key_close;
            lab.pose_far = pose_far;
            lab.pose_close = pose_close;
            lab.camera = camera;
            lab.fov = fov;
            lab.token = token.clone();
            lab.feature_id = feature;
            lab.edge_id = replayed.edge_id;
            lab.gpu_bytes_close = product_close.gpu_bytes;
            lab.product_far = Some(product_far);
            lab.product_close = Some(product_close);
            lab.authority_hash = authority;
            lab.control_vertices = control_vertices;
            lab.control_triangles = control_triangles;
            lab.control_gpu_bytes = Some(control_gpu_bytes);
            lab.revision_before = revision_before;
            lab.panel = panel;
            lab.level_path = level_path.display().to_string();
            lab.split = true;
            lab.stage = if show_sharp { 8 } else { 1 };
            lab.mark = self.frames;
        }
        let source = if show_sharp { "PLANAR CORNER" } else { "OBSERVATION" };
        self.set_curve_hud(&token, !show_sharp, source);
        self.note_curve(&format!("Control mesh id: {}", object_mesh.0));
        self.note_curve(&format!("Legacy cube mesh id: {}", legacy_mesh.0));
        self.note_curve(&format!("Far GPU mesh id: {}", mesh_far.0));
        self.note_curve(&format!("Close GPU mesh id: {}", mesh_close.0));
        self.note_curve("GPU mesh is the object: NO");
        self.note_curve("Realization consulted a stored object mesh: NO");
        self.note_curve("Marker entity: NOT SPAWNED");
        self.note_curve("set_object_mesh called: NO");
        self.note_curve("build_meshlets called: NO");
        self.note_curve("Color pass overrides the Intent Solid: YES");
        self.note_curve("Legacy cube overridden: NO");
        self.note_curve(&format!("World revision after add_mesh: {}", self.engine.world().revision()));
        Ok(())
    }

    fn curve_require_product(&mut self, label: &str, product: &jarvig_core::RoundProduct, feature: &str, token: &str, radius: f64) -> Result<(), String> {
        if product.semantic_feature_id != feature || product.semantic_edge_id != token || product.radius_m.to_bits() != radius.to_bits() {
            return Err(format!("{label} did not realize the saved semantic round"));
        }
        if product.replay_consulted_size_m || product.record_body_consulted || product.object_mesh_consulted {
            return Err(format!("{label} consulted size_m, a stored body, or an object mesh"));
        }
        if product.triangles_discarded_after_construction != 0 {
            return Err(format!("{label} discarded triangles after construction"));
        }
        if product.measured_error_px > jarvig_core::CURVE_ERROR_PX + jarvig_core::CURVE_ERROR_SLACK_PX {
            return Err(format!("{label} measured {:.6} px", product.measured_error_px));
        }
        if product.vertices_constructed != product.mesh.vertex_count() || product.triangles_constructed != product.mesh.index_count() / 3 {
            return Err(format!("{label} packed a different corner than it counted"));
        }
        if product.triangles_constructed <= product.chosen_arc_divisions * 2 {
            return Err(format!("{label} did not add the incident faces to the arc"));
        }
        if product.cpu_bytes != product.gpu_bytes || product.chosen_arc_divisions == 0 || product.chosen_arc_divisions > jarvig_core::CURVE_N_CAP {
            return Err(format!("{label} byte count or division count is not the emitted strip"));
        }
        Ok(())
    }

    fn curve_stage_present(&mut self) -> Result<(), String> {
        if !self.curve_lab.as_ref().unwrap().proved {
            if !self.curve_frame_matches()? {
                if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) > 180 {
                    return Err(format!("the first curve frame did not upload ({})", self.curve_wait_reason()));
                }
                return Ok(());
            }
            self.curve_note_uploads(false)?;
            self.curve_note_passes("simultaneous", false)?;
            let shot = match self.curve_lab.as_ref().unwrap().phase_name.as_str() {
                "reload" => "shot-authored-7-reloaded.png",
                "stay" => "shot-authored-7-reloaded-004.png",
                _ => "shot-authored-7-round.png",
            };
            self.curve_capture(shot)?;
            self.curve_reject_stale()?;
            if self.curve_lab.as_ref().unwrap().discarded_stale_gpu < 3 {
                return Err("the first present rejected fewer than 3 stale uploads".into());
            }
            self.note_curve("First frame: PRESENTED");
            let lab = self.curve_lab.as_mut().unwrap();
            lab.proved = true;
            lab.mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            if !self.curve_frame_matches()? {
                return Err("a later frame substituted a curve buffer".into());
            }
            if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) == 1 && self.curve_lab.as_ref().unwrap().caster_check_pending {
                self.curve_note_passes("cached", true)?;
            }
            return Ok(());
        }
        let phase = self.curve_lab.as_ref().unwrap().phase_name.clone();
        let next = match phase.as_str() {
            "author" => 9,
            "stay" => 10,
            _ => 4,
        };
        self.curve_lab.as_mut().unwrap().stage = next;
        self.curve_lab.as_mut().unwrap().mark = self.frames;
        self.note_curve(&format!("First present held. Next visual stage is {next}."));
        Ok(())
    }

    fn curve_stage_far_gone(&mut self) -> Result<(), String> {
        if self.curve_lab.as_ref().unwrap().mesh_far.is_some() {
            let mesh = self.curve_lab.as_ref().unwrap().mesh_far.ok_or("Far mesh missing")?;
            self.curve_retire_mesh(mesh)?;
            self.curve_lab.as_mut().unwrap().mesh_far = None;
            self.curve_lab.as_mut().unwrap().mark = self.frames;
            self.note_curve("Far destroy requested");
            return Ok(());
        }
        if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let mesh_far = *self.curve_lab.as_ref().unwrap().retired.last().ok_or("Far was not retired")?;
        let mesh_close = self.curve_lab.as_ref().unwrap().mesh_close.ok_or("Close mesh missing")?;
        let control = self.curve_lab.as_ref().unwrap().control_mesh.ok_or("control missing")?;
        let solid = self.curve_lab.as_ref().unwrap().solid.ok_or("solid missing")?;
        let gone = self.renderer.as_ref().unwrap().resident_mesh_bytes(mesh_far).is_none();
        let bytes_close = self.renderer.as_ref().unwrap().resident_mesh_bytes(mesh_close);
        if !gone || bytes_close.is_none() {
            if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) > 90 {
                return Err("destroying Far did not leave Close resident and Far evicted".into());
            }
            return Ok(());
        }
        if bytes_close != Some(self.curve_lab.as_ref().unwrap().gpu_bytes_close) {
            return Err(format!("Close GPU bytes changed to {:?} while Far was destroyed", bytes_close));
        }
        if self.engine.world().object_mesh(solid) != Some(control) {
            return Err("destroying Far changed the object mesh".into());
        }
        if !self.curve_lab.as_ref().unwrap().captured_far_gone {
            let left = self.viewport_view.ok_or("Far view missing")?;
            let right = self.curve_lab.as_ref().unwrap().right_view.ok_or("Close view missing")?;
            let draws = self.renderer.as_ref().unwrap().frame_mesh_draws().to_vec();
            if color_contains(&draws, left, mesh_far) || !color_contains(&draws, left, control) || !color_contains(&draws, right, mesh_close) || color_contains(&draws, right, control) {
                return Err("the frame after destroying Far still mixed the observation meshes".into());
            }
            self.note_curve("Far GPU_bytes after destroy: GONE");
            self.note_curve(&format!("Close GPU_bytes while Far is gone: {}", bytes_close.unwrap()));
            self.note_curve("Authored object after Far destroy: SURVIVES");
            self.curve_capture("shot-authored-7-far-destroyed.png")?;
            self.curve_lab.as_mut().unwrap().captured_far_gone = true;
            self.curve_lab.as_mut().unwrap().mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        self.curve_lab.as_mut().unwrap().stage = 3;
        self.curve_lab.as_mut().unwrap().mark = self.frames;
        self.note_curve("Close destroy is next.");
        Ok(())
    }

    fn curve_stage_close_gone(&mut self) -> Result<(), String> {
        if self.curve_lab.as_ref().unwrap().mesh_close.is_some() {
            let mesh = self.curve_lab.as_ref().unwrap().mesh_close.ok_or("Close mesh missing")?;
            self.curve_retire_mesh(mesh)?;
            self.curve_lab.as_mut().unwrap().mesh_close = None;
            self.curve_lab.as_mut().unwrap().mark = self.frames;
            self.note_curve("Close destroy requested");
            return Ok(());
        }
        if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let retired = self.curve_lab.as_ref().unwrap().retired.clone();
        let gone = retired.iter().all(|mesh| self.renderer.as_ref().unwrap().resident_mesh_bytes(*mesh).is_none());
        if !gone {
            if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) > 90 {
                return Err("Close stayed resident after destroy".into());
            }
            return Ok(());
        }
        let solid = self.curve_lab.as_ref().unwrap().solid.ok_or("solid missing")?;
        let control = self.curve_lab.as_ref().unwrap().control_mesh.ok_or("control missing")?;
        if self.engine.world().object_mesh(solid) != Some(control) {
            return Err("destroying Close changed the object mesh".into());
        }
        let live = self.engine.world().authored_block(solid).ok_or("the authored block disappeared")?;
        let (live_token, live_radius) = active_round(&live)?;
        if live.body.is_some() || live.intent.len() != 66 || live.history.len() != 24 || live_token != self.curve_lab.as_ref().unwrap().token || live_radius.to_bits() != jarvig_core::CURVE_RADIUS_M.to_bits() {
            return Err("the authored object did not survive Close being destroyed".into());
        }
        if !self.curve_lab.as_ref().unwrap().captured_close_gone {
            let left = self.viewport_view.ok_or("Far view missing")?;
            let right = self.curve_lab.as_ref().unwrap().right_view.ok_or("Close view missing")?;
            let draws = self.renderer.as_ref().unwrap().frame_mesh_draws().to_vec();
            if !color_contains(&draws, left, control) || !color_contains(&draws, right, control) || draws.iter().any(|draw| draw.pass == "color" && retired.contains(&draw.mesh)) {
                return Err("the frame after destroying Close did not draw the authored object".into());
            }
            self.note_curve("Close GPU_bytes after destroy: GONE");
            self.note_curve("Authored object after Close destroy: SURVIVES");
            self.curve_capture("shot-authored-7-close-destroyed.png")?;
            self.curve_lab.as_mut().unwrap().captured_close_gone = true;
            self.curve_lab.as_mut().unwrap().mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        self.curve_lab.as_mut().unwrap().stage = 4;
        self.curve_lab.as_mut().unwrap().mark = self.frames;
        self.note_curve("Radius edit is next. No triangle edit is part of it.");
        Ok(())
    }

    fn curve_stage_edit(&mut self) -> Result<(), String> {
        let (level_path, revision_before, old_far, old_close, old_key_far, old_key_close, hash_before, fov, token, feature, edge_id) = {
            let lab = self.curve_lab.as_ref().unwrap();
            (
                lab.level_path.clone(),
                lab.revision_before,
                lab.mesh_far.ok_or("the Far mesh id was not retained")?,
                lab.mesh_close.ok_or("the Close mesh id was not retained")?,
                lab.key_far.clone(),
                lab.key_close.clone(),
                lab.authority_hash,
                lab.fov,
                lab.token.clone(),
                lab.feature_id.clone(),
                lab.edge_id,
            )
        };
        let text = std::fs::read_to_string(&level_path).map_err(|error| error.to_string())?;
        let mut document = jarvig_core::parse_level(&text).map_err(|error| error.to_string())?;
        let current = block_named(&document, "Intent Solid")?;
        let (current_token, current_radius) = active_round(&current)?;
        if current.body.is_some() || current.intent.len() != 66 || current.history.len() != 24 || current_token != token || current_radius.to_bits() != jarvig_core::CURVE_RADIUS_M.to_bits() {
            return Err("the file changed before the radius edit".into());
        }
        if jarvig_core::intent_authority_diagnostic(&current) != "Intent Authority: ELIGIBLE" {
            return Err(jarvig_core::intent_authority_diagnostic(&current));
        }
        let edited = jarvig_core::commit_class_c_intent(&current, jarvig_core::round_entry(&token, jarvig_core::CURVE_EDITED_RADIUS_M)).map_err(|error| error.to_string())?;
        if edited.body.is_some() || edited.intent.len() != 67 || edited.history.len() != 24 {
            return Err("the radius append did not stay body-less at intent 67 and history 24".into());
        }
        let edited_replay = jarvig_core::replay_round(&edited).map_err(|error| error.to_string())?;
        if edited_replay.token != token || edited_replay.edge_id != edge_id || edited_replay.radius_m.to_bits() != jarvig_core::CURVE_EDITED_RADIUS_M.to_bits() {
            return Err("the transaction did not keep the semantic edge and set the active radius to 0.04 m".into());
        }
        if edited.size_m != edited_replay.cache_extent || edited.size_m != jarvig_core::class_c_cache_extent(&edited).map_err(|error| error.to_string())? {
            return Err("the transaction did not write size_m from the replay bounds".into());
        }
        if jarvig_core::feature_id(&edited_replay.token) != feature {
            return Err("the radius edit changed the semantic feature id".into());
        }
        install_block(&mut document, edited);
        let backup = self.curve_artifact_dir().join("Saved").join("Backup");
        jarvig_core::save_level_atomic(std::path::Path::new(&level_path), &backup, &document).map_err(|error| error.to_string())?;
        let parsed_text = std::fs::read_to_string(&level_path).map_err(|error| error.to_string())?;
        if parsed_text.contains("analytic-surface") {
            return Err("the saved edit emitted analytic-surface".into());
        }
        let versions = parametric_versions(&parsed_text)?;
        if versions != [3, 1] && versions != [1, 3] {
            return Err(format!("the saved edit changed ParametricBlock versions to {versions:?}"));
        }
        let parsed_document = jarvig_core::parse_level(&parsed_text).map_err(|error| error.to_string())?;
        let parsed = block_named(&parsed_document, "Intent Solid")?;
        let legacy = block_named(&parsed_document, "Legacy Cube")?;
        if parsed.body.is_some() || legacy.body.is_none() || parsed.intent.len() != 67 || parsed.history.len() != 24 {
            return Err("the parsed edit did not keep the absent body, the legacy body, intent 67, and history 24".into());
        }
        let rounds: Vec<_> = parsed.intent.iter().filter(|entry| matches!(entry.payload, jarvig_core::IntentPayload::Round { .. })).collect();
        if rounds.len() != 2 || rounds[0].groups != Some(vec![vec![token.clone()]]) || rounds[1].groups != Some(vec![vec![token.clone()]]) {
            return Err("the parsed tape does not keep one semantic edge on both round operations".into());
        }
        let parsed_replay = jarvig_core::replay_round(&parsed).map_err(|error| error.to_string())?;
        if parsed_replay.token != token || parsed_replay.edge_id != edge_id || parsed_replay.radius_m.to_bits() != jarvig_core::CURVE_EDITED_RADIUS_M.to_bits() {
            return Err("reload did not resolve the same semantic edge at 0.04 m".into());
        }
        if parsed.size_m != parsed_replay.cache_extent {
            return Err(format!("parsed size_m {} is not the replay cache {}", size_bits(parsed.size_m), size_bits(parsed_replay.cache_extent)));
        }
        let diagnostic = jarvig_core::intent_authority_diagnostic(&parsed);
        if diagnostic != "Intent Authority: ELIGIBLE" {
            return Err(diagnostic);
        }
        let edited_hash = jarvig_core::authored_authority_hash(&parsed).map_err(|error| error.to_string())?;
        if edited_hash == hash_before {
            return Err("the saved radius edit did not change the authority hash".into());
        }
        let far_camera = jarvig_core::presentation_camera(
            &parsed_replay.fillet,
            jarvig_core::CURVE_PRESENT_FAR_M,
            VIEW_FAR_WIDTH as f32,
            VIEW_HEIGHT as f32,
            fov,
            jarvig_core::CURVE_ERROR_PX,
        )
        .map_err(|error| error.to_string())?;
        let close_camera = jarvig_core::presentation_camera(
            &parsed_replay.fillet,
            jarvig_core::CURVE_PRESENT_CLOSE_M,
            VIEW_CLOSE_WIDTH as f32,
            VIEW_HEIGHT as f32,
            fov,
            jarvig_core::CURVE_ERROR_PX,
        )
        .map_err(|error| error.to_string())?;
        if far_camera.vertical_fov_radians.to_bits() != fov.to_bits() || far_camera.viewport_width != VIEW_FAR_WIDTH as f32 || close_camera.viewport_width != VIEW_CLOSE_WIDTH as f32 {
            return Err("the second present moved the field of view or a viewport".into());
        }
        let product_far = jarvig_core::realize_round_corner(&parsed, &far_camera, "Far", 2).map_err(|error| format!("edited Far stopped: {error}"))?;
        let product_close = jarvig_core::realize_round_corner(&parsed, &close_camera, "Close", 2).map_err(|error| format!("edited Close stopped: {error}"))?;
        if product_close.chosen_arc_divisions <= product_far.chosen_arc_divisions {
            return Err(format!("edited Close arc divisions {} are not finer than Far {}", product_close.chosen_arc_divisions, product_far.chosen_arc_divisions));
        }
        self.curve_require_product("Edited Far", &product_far, &feature, &token, jarvig_core::CURVE_EDITED_RADIUS_M)?;
        self.curve_require_product("Edited Close", &product_close, &feature, &token, jarvig_core::CURVE_EDITED_RADIUS_M)?;
        let before_far = self.curve_lab.as_ref().unwrap().product_far.clone().ok_or("Far missing")?;
        if product_far.position_hash == before_far.position_hash || product_close.position_hash == self.curve_lab.as_ref().unwrap().product_close.as_ref().ok_or("Close missing")?.position_hash {
            return Err("the radius edit left an observation position hash unchanged".into());
        }
        self.note_curve("Radius edit written by the level writer: YES");
        self.note_curve("Triangle edit during the radius change: NO");
        self.note_curve("body remains absent: YES");
        self.note_curve(&format!("semantic_feature_id after edit: {}", product_far.semantic_feature_id));
        self.note_curve(&format!("semantic_edge_id after edit: {}", product_far.semantic_edge_id));
        self.note_curve(&format!("concrete edge id after edit: {}", parsed_replay.edge_id));
        self.note_curve("semantic feature identity unchanged: YES");
        self.note_curve("semantic edge identity unchanged: YES");
        self.note_curve(&format!("radius after edit: {:.5} m bits {:016x}", parsed_replay.radius_m, parsed_replay.radius_m.to_bits()));
        self.note_curve(&format!("authority_hash after edit: {edited_hash:016x}"));
        self.note_curve("authority_revision: 2");
        self.note_curve("authority hash changed: YES");
        self.note_curve(&format!("size_m bits after edit: {}", size_bits(parsed.size_m)));
        self.note_curve("size_m rewritten from replay bounds: YES");
        self.note_curve("Intent length after edit: 67");
        self.note_curve("History length after edit: 24");
        self.note_curve(&diagnostic);
        self.note_curve("Second realization source: parsed level file");
        self.note_curve(&format!("Edited Far distance_m: {:.6}", jarvig_core::CURVE_PRESENT_FAR_M));
        self.note_curve(&format!("Edited Close distance_m: {:.6}", jarvig_core::CURVE_PRESENT_CLOSE_M));
        self.note_curve(&format!("Edited Far arc divisions: {}", product_far.chosen_arc_divisions));
        self.note_curve(&format!("Edited Close arc divisions: {}", product_close.chosen_arc_divisions));
        self.note_curve(&format!("Edited Far eye_local: [{:.6}, {:.6}, {:.6}]", far_camera.eye[0], far_camera.eye[1], far_camera.eye[2]));
        self.note_curve(&format!("Edited Close eye_local: [{:.6}, {:.6}, {:.6}]", close_camera.eye[0], close_camera.eye[1], close_camera.eye[2]));
        let (left, right, entity) = {
            let lab = self.curve_lab.as_ref().unwrap();
            (self.viewport_view.ok_or("Far view missing")?, lab.right_view.ok_or("Close view missing")?, lab.solid.unwrap())
        };
        let discarded_before = self.curve_lab.as_ref().unwrap().discarded_stale_gpu;
        let new_key_far = curve_key(&product_far);
        let new_key_close = curve_key(&product_close);
        self.curve_bind_if_current(left, entity, old_far, &old_key_far, &new_key_far)?;
        self.curve_bind_if_current(right, entity, old_close, &old_key_close, &new_key_close)?;
        if self.curve_lab.as_ref().unwrap().discarded_stale_gpu < discarded_before + 2 {
            return Err("the pre-edit observation meshes were not rejected after the radius edit".into());
        }
        self.note_curve("Old observation meshes stale: YES");
        self.note_curve("Stale uploads rejected: YES");
        let mesh_far = self.engine.world_mut().add_mesh(product_far.mesh.clone());
        let mesh_close = self.engine.world_mut().add_mesh(product_close.mesh.clone());
        if self.engine.world().revision() != revision_before {
            return Err("the edited upload revised the world".into());
        }
        if mesh_far == old_far || mesh_close == old_close || mesh_far == mesh_close {
            return Err("the edited realization reused a pre-edit mesh id".into());
        }
        let solid_pose = self.engine.world().entity_world_pose(entity).map_err(|error| error.to_string())?;
        let pose_far = curve_pose(solid_pose, far_camera.eye, far_camera.forward)?;
        let pose_close = curve_pose(solid_pose, close_camera.eye, close_camera.forward)?;
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.set_view_mesh_override(left, entity, Some(mesh_far)).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(right, entity, Some(mesh_close)).map_err(|error| error.to_string())?;
        }
        self.note_curve(&format!("{}gpu_mesh_id: {}", jarvig_core::format_round_product(&product_far), mesh_far.0));
        self.note_curve(&format!("{}gpu_mesh_id: {}", jarvig_core::format_round_product(&product_close), mesh_close.0));
        let edited_divisions_far = product_far.chosen_arc_divisions;
        let edited_divisions_close = product_close.chosen_arc_divisions;
        {
            let lab = self.curve_lab.as_mut().unwrap();
            lab.mesh_far = Some(mesh_far);
            lab.mesh_close = Some(mesh_close);
            lab.corner_far = Some(mesh_far);
            lab.corner_close = Some(mesh_close);
            lab.observation_meshes.extend([mesh_far, mesh_close]);
            lab.key_far = new_key_far;
            lab.key_close = new_key_close;
            lab.pose_far = pose_far;
            lab.pose_close = pose_close;
            lab.gpu_bytes_close = product_close.gpu_bytes;
            lab.product_far = Some(product_far);
            lab.product_close = Some(product_close);
            lab.divisions_far = edited_divisions_far;
            lab.divisions_close = edited_divisions_close;
            lab.radius_m = jarvig_core::CURVE_EDITED_RADIUS_M;
            lab.edited_hash = edited_hash;
            lab.discarded_before_edit = discarded_before;
            lab.stage = 5;
            lab.mark = self.frames;
        }
        self.curve_retire_mesh(old_far)?;
        self.curve_retire_mesh(old_close)?;
        self.note_curve("Pre-edit radius meshes retired after the view switched: YES");
        self.note_curve(&format!("Edited Far GPU mesh id: {}", mesh_far.0));
        self.note_curve(&format!("Edited Close GPU mesh id: {}", mesh_close.0));
        self.note_curve(&format!("World revision after edit: {}", self.engine.world().revision()));
        self.note_curve("Live world edited in place: NO");
        let token = self.curve_lab.as_ref().unwrap().token.clone();
        self.set_curve_hud(&token, true, "OBSERVATION");
        Ok(())
    }

    fn curve_stage_sharp(&mut self) -> Result<(), String> {
        if !self.curve_frame_matches()? {
            if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) > 180 {
                return Err(format!("the sharp corner did not draw ({})", self.curve_wait_reason()));
            }
            return Ok(());
        }
        if !self.curve_lab.as_ref().unwrap().proved {
            let lab = self.curve_lab.as_mut().unwrap();
            lab.proved = true;
            lab.mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        self.curve_capture("shot-authored-7-before.png")?;
        self.note_curve("Sharp corner: PRESENTED");
        let (left, right, entity, corner_far, corner_close, token) = {
            let lab = self.curve_lab.as_ref().unwrap();
            (
                self.viewport_view.ok_or("Far view missing")?,
                lab.right_view.ok_or("Close view missing")?,
                lab.solid.unwrap(),
                lab.corner_far.unwrap(),
                lab.corner_close.unwrap(),
                lab.token.clone(),
            )
        };
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.set_view_mesh_override(left, entity, Some(corner_far)).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(right, entity, Some(corner_close)).map_err(|error| error.to_string())?;
        }
        {
            let lab = self.curve_lab.as_mut().unwrap();
            lab.mesh_far = Some(corner_far);
            lab.mesh_close = Some(corner_close);
            lab.proved = false;
            lab.stage = 1;
            lab.mark = self.frames;
        }
        self.set_curve_hud(&token, true, "OBSERVATION");
        self.note_curve("Round corner is now the observation. The saved tape already holds the round.");
        Ok(())
    }

    fn curve_stage_wire(&mut self) -> Result<(), String> {
        let (wire_far, wire_close, showing_wire) = {
            let lab = self.curve_lab.as_ref().unwrap();
            (lab.wire_far.unwrap(), lab.wire_close.unwrap(), lab.mesh_far == lab.wire_far)
        };
        if !showing_wire {
            let (left, right, entity) = {
                let lab = self.curve_lab.as_ref().unwrap();
                (self.viewport_view.ok_or("Far view missing")?, lab.right_view.ok_or("Close view missing")?, lab.solid.unwrap())
            };
            {
                let renderer = self.renderer.as_mut().unwrap();
                renderer.set_view_mesh_override(left, entity, Some(wire_far)).map_err(|error| error.to_string())?;
                renderer.set_view_mesh_override(right, entity, Some(wire_close)).map_err(|error| error.to_string())?;
            }
            let lab = self.curve_lab.as_mut().unwrap();
            lab.mesh_far = Some(wire_far);
            lab.mesh_close = Some(wire_close);
            lab.proved = false;
            lab.mark = self.frames;
            self.note_curve("Wireframe of the generated arc is next.");
            return Ok(());
        }
        if !self.curve_frame_matches()? {
            if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) > 180 {
                return Err(format!("the arc wireframe did not draw ({})", self.curve_wait_reason()));
            }
            return Ok(());
        }
        if !self.curve_lab.as_ref().unwrap().proved {
            let lab = self.curve_lab.as_mut().unwrap();
            lab.proved = true;
            lab.mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            return Ok(());
        }
        self.curve_capture("shot-authored-7-wireframe.png")?;
        self.note_curve("Arc wireframe: PRESENTED");
        self.curve_lab.as_mut().unwrap().stage = 10;
        self.curve_lab.as_mut().unwrap().mark = self.frames;
        Ok(())
    }

    fn curve_stage_author_done(&mut self) -> Result<(), String> {
        self.note_curve("Author phase closed. The editor will exit and the next launch reloads this 0.05 m tape.");
        self.curve_write_phase("reload")?;
        self.curve_write_phase_report();
        self.curve_lab.as_mut().unwrap().stage = 15;
        unsafe { PostQuitMessage(0); }
        Ok(())
    }

    fn curve_stage_reload_done(&mut self) -> Result<(), String> {
        let saved_ok = self.curve_saved_radius(jarvig_core::CURVE_EDITED_RADIUS_M, 67)?;
        let stale = self.curve_lab.as_ref().unwrap().discarded_stale_gpu;
        let presented = self.curve_lab.as_ref().unwrap().report.contains("First frame: PRESENTED") && self.curve_lab.as_ref().unwrap().report.contains("Second frame: PRESENTED");
        if !saved_ok || stale < 5 || !presented {
            return Err(format!("reload phase saved {saved_ok}, stale {stale}, presented {presented}"));
        }
        self.note_curve("Reload phase closed. The saved tape is the 0.04 m round. The next launch keeps the editor open.");
        self.curve_write_phase("stay")?;
        self.curve_write_phase_report();
        self.curve_lab.as_mut().unwrap().stage = 15;
        unsafe { PostQuitMessage(0); }
        Ok(())
    }

    fn curve_stage_release(&mut self) -> Result<(), String> {
        let (right, left, entity, mesh_close, pose_close) = {
            let lab = self.curve_lab.as_ref().unwrap();
            (
                lab.right_view.ok_or("Close view missing")?,
                self.viewport_view.ok_or("the perspective view is missing")?,
                lab.solid.unwrap(),
                lab.corner_close.ok_or("the close corner mesh is missing")?,
                lab.pose_close,
            )
        };
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.destroy_view(right).map_err(|error| error.to_string())?;
            renderer.set_view_pixel_rect(left, None).map_err(|error| error.to_string())?;
            renderer.set_view_mesh_override(left, entity, Some(mesh_close)).map_err(|error| error.to_string())?;
        }
        let forward = pose_close.rotation.rotate(jarvig_core::Vec3::new(0.0, 0.0, -1.0));
        let distance = jarvig_core::CURVE_PRESENT_CLOSE_M * (1.0 + jarvig_core::CURVE_PRESENT_LATERAL * jarvig_core::CURVE_PRESENT_LATERAL).sqrt();
        let pivot = pose_close.translation + forward.scale(distance);
        if let Some(controller) = self.editor_camera.as_mut() {
            controller.reset_to(pose_close);
            controller.orbit_pivot = pivot;
            controller.orbit_distance = distance;
            controller.mode = super::camera::CameraNavMode::Orbit;
            controller.near_m = jarvig_core::CURVE_PRESENT_NEAR_M as f32;
            controller.speed_m_s = 0.15;
        }
        self.push_editor_camera()?;
        {
            let lab = self.curve_lab.as_mut().unwrap();
            lab.right_view = None;
            lab.mesh_far = Some(mesh_close);
            lab.mesh_close = None;
            lab.split = false;
            lab.stage = 15;
        }
        let token = self.curve_lab.as_ref().unwrap().token.clone();
        self.set_curve_hud(&token, true, "OBSERVATION");
        self.note_curve("Editor left open on the reloaded round. Orbit and fly stay on this observation.");
        self.curve_write_phase_report();
        Ok(())
    }

    fn curve_saved_radius(&self, radius: f64, intent_len: usize) -> Result<bool, String> {
        let path = self.curve_lab.as_ref().unwrap().level_path.clone();
        let text = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        if text.contains("analytic-surface") {
            return Ok(false);
        }
        let document = jarvig_core::parse_level(&text).map_err(|error| error.to_string())?;
        let record = block_named(&document, "Intent Solid")?;
        let legacy = block_named(&document, "Legacy Cube")?;
        let replayed = jarvig_core::replay_round(&record).map_err(|error| error.to_string())?;
        Ok(record.body.is_none()
            && legacy.body.is_some()
            && record.intent.len() == intent_len
            && record.history.len() == 24
            && replayed.radius_m.to_bits() == radius.to_bits()
            && replayed.token == self.curve_lab.as_ref().unwrap().token
            && jarvig_core::intent_authority_diagnostic(&record) == "Intent Authority: ELIGIBLE")
    }

    fn curve_phase_name(&self) -> String {
        let text = std::fs::read_to_string(self.curve_artifact_dir().join("VISUAL-PHASE.txt")).unwrap_or_default();
        match text.trim() {
            "reload" => "reload".into(),
            "stay" => "stay".into(),
            _ => "author".into(),
        }
    }

    fn curve_write_phase(&self, phase: &str) -> Result<(), String> {
        let path = self.curve_artifact_dir().join("VISUAL-PHASE.txt");
        std::fs::write(&path, format!("{phase}\n")).map_err(|error| error.to_string())
    }

    fn curve_write_phase_report(&mut self) {
        self.curve_write_report();
        let Some(lab) = self.curve_lab.as_ref() else { return };
        let path = self.curve_artifact_dir().join(format!("REPORT-AUTHORED-7-{}.txt", lab.phase_name));
        let _ = std::fs::write(path, &lab.report);
    }

    fn set_curve_hud(&mut self, token: &str, round_on: bool, source: &str) {
        let (radius, far_n, close_n) = {
            let lab = self.curve_lab.as_ref().unwrap();
            (lab.radius_m, lab.divisions_far, lab.divisions_close)
        };
        let feature = if round_on { "YES" } else { "NO" };
        let divisions = if round_on { format!("{far_n} far, {close_n} close") } else { "0".into() };
        let text = format!(
            "AUTHORED GEOMETRY\nRound feature: {feature}\nSemantic edge: {token}\nRadius: {radius:.3} m\nStored body: NO\nPersistent round intent: YES\nRender geometry source: {source}\nGenerated arc divisions: {divisions}"
        );
        self.set_status(&format!(
            "AUTHORED GEOMETRY  Round {feature}  edge {token}  radius {radius:.3} m  body NO  intent YES  source {source}  arc {divisions}"
        ));
        self.curve_lab.as_mut().unwrap().hud_text = text.clone();
        if self.curve_lab.as_ref().unwrap().hud.is_null() {
            match create_curve_hud(self.frame) {
                Ok(hwnd) => self.curve_lab.as_mut().unwrap().hud = hwnd,
                Err(error) => self.note_curve(&format!("HUD create failed: {error}")),
            }
        }
        let hwnd = self.curve_lab.as_ref().unwrap().hud;
        if !hwnd.is_null() {
            let wide = curve_wide(&text);
            unsafe {
                SetWindowTextW(hwnd, wide.as_ptr());
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
        }
        self.place_curve_hud();
    }

    fn place_curve_hud(&mut self) {
        let hwnd = self.curve_lab.as_ref().map(|lab| lab.hud).unwrap_or(std::ptr::null_mut());
        if hwnd.is_null() || self.frame.is_null() {
            return;
        }
        let panel = self.panel_hwnd(PERSPECTIVE);
        if panel.is_null() {
            return;
        }
        unsafe {
            let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            if GetWindowRect(panel, &mut rect) == 0 {
                return;
            }
            SetWindowPos(hwnd, std::ptr::null_mut(), rect.left + 16, rect.top + 16, 640, 196, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            UpdateWindow(hwnd);
        }
    }

    fn curve_stage_edited(&mut self) -> Result<(), String> {
        if !self.curve_lab.as_ref().unwrap().edited_proved {
            if !self.curve_frame_matches()? {
                if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) > 180 {
                    return Err(format!("the edited curve frame did not upload ({})", self.curve_wait_reason()));
                }
                return Ok(());
            }
            self.curve_note_uploads(true)?;
            self.curve_note_passes("edited", false)?;
            self.curve_capture("shot-authored-7-edited.png")?;
            self.note_curve("Second frame: PRESENTED");
            let lab = self.curve_lab.as_mut().unwrap();
            lab.edited_proved = true;
            lab.mark = self.frames;
            return Ok(());
        }
        if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) < HOLD_SHOT {
            if !self.curve_frame_matches()? {
                return Err("a later edited frame substituted a curve buffer".into());
            }
            if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) == 1 && self.curve_lab.as_ref().unwrap().caster_check_pending {
                self.curve_note_passes("edited-cached", true)?;
            }
            return Ok(());
        }
        self.curve_lab.as_mut().unwrap().stage = 6;
        self.curve_lab.as_mut().unwrap().mark = self.frames;
        Ok(())
    }

    fn curve_stage_intact(&mut self) -> Result<(), String> {
        if self.frames.saturating_sub(self.curve_lab.as_ref().unwrap().mark) < 2 {
            return Ok(());
        }
        let (object_ok, counts_ok, presented, graphics_ok, saved_ok, live_ok, mesh_same, ordinary_same, control_match, legacy_same, revision_same, main_len, acquires_first, presents_first, acquires_second, presents_second, discarded, control_text) = {
            let lab = self.curve_lab.as_ref().unwrap();
            let solid = lab.solid.ok_or("solid missing")?;
            let legacy = lab.legacy.ok_or("legacy missing")?;
            let control = lab.control_mesh.ok_or("control missing")?;
            let legacy_mesh = lab.legacy_mesh.ok_or("legacy mesh missing")?;
            let mesh_same = self.engine.world().object_mesh(solid) == Some(control);
            let legacy_same = self.engine.world().object_mesh(legacy) == Some(legacy_mesh);
            let ordinary_same = self.engine.world().meshes().get(control).is_some_and(|mesh| mesh.vertex_count() == lab.control_vertices && mesh.index_count() / 3 == lab.control_triangles);
            let live = self.engine.world().authored_block(solid);
            let live_ok = live.as_ref().is_some_and(|record| {
                record.body.is_none()
                    && record.intent.len() == 66
                    && record.history.len() == 24
                    && active_round(record).ok().is_some_and(|(token, radius)| token == lab.token && radius.to_bits() == jarvig_core::CURVE_RADIUS_M.to_bits())
            });
            let control_after = self.renderer.as_ref().unwrap().resident_mesh_bytes(control);
            let control_match = control_after == lab.control_gpu_bytes && control_after == Some(1512);
            let saved_text = std::fs::read_to_string(&lab.level_path).unwrap_or_default();
            let saved = jarvig_core::parse_level(&saved_text).ok();
            let saved_record = saved.as_ref().and_then(|document| block_named(document, "Intent Solid").ok());
            let saved_legacy = saved.as_ref().and_then(|document| block_named(document, "Legacy Cube").ok());
            let saved_ok = saved_record.as_ref().is_some_and(|record| {
                record.body.is_none()
                    && record.history.len() == 24
                    && record.intent.len() == 67
                    && jarvig_core::replay_round(record).ok().is_some_and(|replayed| {
                        replayed.token == lab.token
                            && replayed.edge_id == lab.edge_id
                            && replayed.radius_m.to_bits() == jarvig_core::CURVE_EDITED_RADIUS_M.to_bits()
                            && record.size_m == replayed.cache_extent
                            && jarvig_core::feature_id(&replayed.token) == lab.feature_id
                    })
                    && jarvig_core::intent_authority_diagnostic(record) == "Intent Authority: ELIGIBLE"
            }) && saved_legacy.as_ref().is_some_and(|record| record.body.is_some())
                && !saved_text.contains("analytic-surface")
                && parametric_versions(&saved_text).ok().is_some_and(|versions| versions == [3, 1] || versions == [1, 3]);
            let main_len = std::fs::metadata(jarvig_core::authored_main_level()).map(|meta| meta.len()).unwrap_or(0);
            let main_same = main_len == jarvig_core::AUTHORED_MAIN_LEVEL_BYTES;
            let revision_same = self.engine.world().revision() == lab.revision_before;
            let graphics_ok = self.graphics_vendor == 0x8086 && self.graphics_device == 0x9b41 && self.graphics_api == "dx12";
            let presented = self.renderer.as_ref().unwrap().presented_frames() > 0;
            let counts_ok = lab.acquires_first == 1 && lab.presents_first == 1 && lab.acquires_second == 1 && lab.presents_second == 1;
            let stale_ok = lab.discarded_stale_gpu >= lab.discarded_before_edit + 2 && lab.discarded_stale_gpu >= 5;
            let workers_ok = self.jobs.worker_count() == 2 && self.einstein_jobs.worker_count() == 1;
            let hash_moved = lab.edited_hash != 0 && lab.edited_hash != lab.authority_hash;
            let panel_ok = lab.panel.0 >= REQUIRED_WIDTH && lab.panel.1 >= VIEW_HEIGHT;
            let products_ok = lab.product_far.as_ref().is_some_and(|product| product.triangles_discarded_after_construction == 0)
                && lab.product_close.as_ref().is_some_and(|product| product.triangles_discarded_after_construction == 0);
            let object_ok = mesh_same
                && legacy_same
                && ordinary_same
                && live_ok
                && control_match
                && saved_ok
                && main_same
                && revision_same
                && graphics_ok
                && presented
                && counts_ok
                && stale_ok
                && workers_ok
                && hash_moved
                && panel_ok
                && products_ok
                && !self.engine.intent_authority_experiment();
            let control_text = control_after.map(|value| value.to_string()).unwrap_or_else(|| "GONE".into());
            (
                object_ok,
                counts_ok,
                presented,
                graphics_ok,
                saved_ok,
                live_ok,
                mesh_same,
                ordinary_same,
                control_match,
                legacy_same,
                revision_same,
                main_len,
                lab.acquires_first,
                lab.presents_first,
                lab.acquires_second,
                lab.presents_second,
                lab.discarded_stale_gpu,
                control_text,
            )
        };
        self.curve_capture("shot-authored-7-object.png")?;
        self.note_curve(&format!("Editor frame: {}", self.frames));
        self.note_curve(&format!("Presented frames: {}", self.renderer.as_ref().unwrap().presented_frames()));
        self.note_curve(&format!("Frame: {}", if presented && counts_ok { "PRESENTED" } else { "NOT PRESENTED" }));
        self.note_curve(&format!("Graphics is Intel UHD DX12 8086:9b41: {}", yes_no(graphics_ok)));
        self.note_curve(&format!("Acquires on the first proof frame: {acquires_first}"));
        self.note_curve(&format!("Presents on the first proof frame: {presents_first}"));
        self.note_curve(&format!("Acquires on the second proof frame: {acquires_second}"));
        self.note_curve(&format!("Presents on the second proof frame: {presents_second}"));
        self.note_curve(&format!("Saved round tape intact: {}", yes_no(saved_ok)));
        self.note_curve(&format!("Live world kept the 0.05 m tape: {}", yes_no(live_ok)));
        self.note_curve(&format!("Ordinary control mesh unchanged: {}", yes_no(mesh_same && ordinary_same && control_match)));
        self.note_curve(&format!("Legacy cube mesh unchanged: {}", yes_no(legacy_same)));
        self.note_curve(&format!("World revision unchanged: {}", yes_no(revision_same)));
        self.note_curve(&format!("Control mesh GPU_bytes after: {control_text}"));
        self.note_curve(&format!("Main.jarviglevel bytes: {main_len}"));
        self.note_curve(&format!("stale uploads rejected: {discarded}"));
        self.note_curve("Intent-authority experiment: OFF");
        self.note_curve("ADR-0074 accepted: NO");
        self.note_curve("Renderer replacement: NO");
        self.note_curve("analytic-surface whitelisted: NO");
        let passed = object_ok
            && self.curve_lab.as_ref().unwrap().report.contains("expected_gpu_bytes = GpuMesh.bytes = create_buffer bytes: YES")
            && self.curve_lab.as_ref().unwrap().report.contains("Second frame: PRESENTED")
            && self.curve_lab.as_ref().unwrap().report.contains("First frame: PRESENTED")
            && self.curve_lab.as_ref().unwrap().report.contains("Authored object after Far destroy: SURVIVES")
            && self.curve_lab.as_ref().unwrap().report.contains("Authored object after Close destroy: SURVIVES")
            && self.curve_lab.as_ref().unwrap().report.contains("replay consulted size_m: NO")
            && self.curve_lab.as_ref().unwrap().report.contains("size_m rewritten from replay bounds: YES")
            && self.curve_lab.as_ref().unwrap().report.contains("semantic feature identity unchanged: YES")
            && self.curve_lab.as_ref().unwrap().report.contains("semantic edge identity unchanged: YES");
        if passed {
            self.note_curve("Earned: A user-authored JARVIG object persisted a semantic curved boundary operation instead of tessellated geometry; after save/reload, independent observations directly synthesized different-resolution GPU realizations of that same editable feature.");
        }
        self.note_curve(&format!("Experiment 7: {}", if passed { "PASS" } else { "FAIL" }));
        self.curve_write_report();
        self.curve_lab.as_mut().unwrap().stage = 15;
        if !passed {
            return Err("Experiment 7 did not meet the presented-frame success condition".into());
        }
        unsafe { PostQuitMessage(0); }
        Ok(())
    }

    fn curve_note_uploads(&mut self, second: bool) -> Result<(), String> {
        let (mesh_far, mesh_close, product_far, product_close, control) = {
            let lab = self.curve_lab.as_ref().unwrap();
            (lab.mesh_far.unwrap(), lab.mesh_close.unwrap(), lab.product_far.clone().unwrap(), lab.product_close.clone().unwrap(), lab.control_mesh.unwrap())
        };
        let record_far = self.renderer.as_ref().unwrap().mesh_upload_record(mesh_far).ok_or("Far was not uploaded")?;
        let record_close = self.renderer.as_ref().unwrap().mesh_upload_record(mesh_close).ok_or("Close was not uploaded")?;
        let prove = |label: &str, product: &jarvig_core::RoundProduct, record: jarvig_renderer::MeshUploadRecord| -> Result<(), String> {
            if record.vertex_create_bytes != product.packed_vertex_bytes || record.index_create_bytes != product.packed_index_bytes || record.gpu_bytes != product.gpu_bytes {
                return Err(format!("{label} create_buffer bytes did not match the provenance total"));
            }
            if record.vertex_create_bytes + record.index_create_bytes != record.gpu_bytes || product.cpu_bytes != product.gpu_bytes {
                return Err(format!("{label} GpuMesh.bytes is not the sum of the create_buffer sizes"));
            }
            let uploaded = record.index_create_bytes / u64::from(product.index_format.byte_size()) / 3;
            if uploaded != u64::from(product.triangles_constructed) || product.triangles_discarded_after_construction != 0 {
                return Err(format!("{label} uploaded a different triangle count"));
            }
            Ok(())
        };
        let label_far = if second { "Edited Far" } else { "Far" };
        let label_close = if second { "Edited Close" } else { "Close" };
        prove(label_far, &product_far, record_far)?;
        prove(label_close, &product_close, record_close)?;
        let control_bytes = self.renderer.as_ref().unwrap().resident_mesh_bytes(control);
        if control_bytes != Some(1512) {
            return Err(format!("the ordinary control left 1512 GPU bytes ({control_bytes:?})"));
        }
        self.note_curve(&format!("{label_far} GpuMesh.bytes: {}", record_far.gpu_bytes));
        self.note_curve(&format!("{label_far} create_buffer bytes: {}", record_far.vertex_create_bytes + record_far.index_create_bytes));
        self.note_curve(&format!("{label_close} GpuMesh.bytes: {}", record_close.gpu_bytes));
        self.note_curve(&format!("{label_close} create_buffer bytes: {}", record_close.vertex_create_bytes + record_close.index_create_bytes));
        self.note_curve("expected_gpu_bytes = GpuMesh.bytes = create_buffer bytes: YES");
        self.note_curve("triangles_discarded_after_construction: 0");
        self.note_curve("Ordinary control GPU_bytes: 1512");
        let acquires = self.renderer.as_ref().unwrap().acquires_last_frame();
        let presents = self.renderer.as_ref().unwrap().presents_last_frame();
        self.note_curve(&format!("Acquires this frame: {acquires}"));
        self.note_curve(&format!("Presents this frame: {presents}"));
        if acquires != 1 || presents != 1 {
            return Err("the curve frame did not use one acquire and one present".into());
        }
        let lab = self.curve_lab.as_mut().unwrap();
        if second {
            lab.acquires_second = acquires;
            lab.presents_second = presents;
        } else {
            lab.acquires_first = acquires;
            lab.presents_first = presents;
        }
        lab.gpu_bytes_close = record_close.gpu_bytes;
        Ok(())
    }

    fn curve_note_passes(&mut self, label: &str, require_recorded: bool) -> Result<(), String> {
        let draws = self.renderer.as_ref().unwrap().frame_mesh_draws().to_vec();
        let control = self.curve_lab.as_ref().unwrap().control_mesh.unwrap();
        let legacy = self.curve_lab.as_ref().unwrap().legacy_mesh.unwrap();
        let decoy = self.curve_lab.as_ref().unwrap().decoy.unwrap();
        let observation = self.curve_lab.as_ref().unwrap().observation_meshes.clone();
        let shadow = pass_meshes(&draws, "shadow");
        let shadow_cached = pass_meshes(&draws, "shadow-cached");
        let contact = pass_meshes(&draws, "contact");
        let probe = pass_meshes(&draws, "probe");
        let casters = if shadow.is_empty() { shadow_cached.clone() } else { shadow.clone() };
        self.note_curve(&format!("{label} shadow casters: {}", mesh_list(&casters)));
        let forbidden = |list: &[MeshId]| list.iter().any(|mesh| observation.contains(mesh) || *mesh == decoy);
        if forbidden(&shadow) || forbidden(&shadow_cached) || forbidden(&contact) || forbidden(&probe) {
            return Err("an observation mesh was drawn by shadow, contact, or probe capture".into());
        }
        self.note_curve("Observation mesh in shadow draws: NO");
        self.note_curve("Observation mesh in contact draws: NO");
        self.note_curve("Observation mesh in probe draws: NO");
        if casters.is_empty() && !require_recorded {
            let solid = self.curve_lab.as_ref().unwrap().solid.ok_or("the ordinary control entity is missing")?;
            let snapshot = self.engine.world().extract(jarvig_core::RenderFrameId(1)).map_err(|error| error.to_string())?;
            let casts = snapshot.instances().iter().find(|instance| instance.entity == solid).is_some_and(|instance| instance.cast_shadows && instance.mesh == control);
            if !casts {
                return Err("the ordinary control mesh does not cast shadows".into());
            }
            self.note_curve("Ordinary control cast_shadows: YES");
            self.note_curve("Shadow caster list on the rebuild frame: NOT RECORDED");
            self.curve_lab.as_mut().unwrap().caster_check_pending = true;
            return Ok(());
        }
        if !casters.contains(&control) || !casters.contains(&legacy) {
            return Err("the ordinary control mesh or the legacy cube was not a shadow caster".into());
        }
        self.note_curve("Ordinary control mesh in shadow casters: YES");
        self.note_curve("Legacy cube in shadow casters: YES");
        self.curve_lab.as_mut().unwrap().caster_check_pending = false;
        Ok(())
    }

    fn curve_wait_reason(&self) -> String {
        let Some(renderer) = self.renderer.as_ref() else {
            return "renderer missing".into();
        };
        let Some(lab) = self.curve_lab.as_ref() else {
            return "lab missing".into();
        };
        let rect = |view: Option<jarvig_renderer::RenderViewId>| {
            view.and_then(|view| renderer.viewport(view).ok().flatten()).map(|rect| format!("{}x{}@{},{}", rect.width, rect.height, rect.x, rect.y)).unwrap_or_else(|| "none".into())
        };
        let uploaded = |mesh: Option<MeshId>| mesh.and_then(|mesh| renderer.mesh_upload_record(mesh)).is_some();
        format!("Far {} Close {} Far_uploaded {} Close_uploaded {}", rect(self.viewport_view), rect(lab.right_view), uploaded(lab.mesh_far), uploaded(lab.mesh_close))
    }

    fn curve_frame_matches(&self) -> Result<bool, String> {
        let Some(renderer) = self.renderer.as_ref() else {
            return Ok(false);
        };
        let left = self.viewport_view.ok_or("Far view missing")?;
        let right = match self.curve_lab.as_ref().unwrap().right_view {
            Some(view) => view,
            None => return Ok(false),
        };
        let mesh_far = self.curve_lab.as_ref().unwrap().mesh_far;
        let mesh_close = self.curve_lab.as_ref().unwrap().mesh_close;
        let control = self.curve_lab.as_ref().unwrap().control_mesh.ok_or("control missing")?;
        let decoy = self.curve_lab.as_ref().unwrap().decoy.ok_or("decoy missing")?;
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

    fn curve_reject_stale(&mut self) -> Result<(), String> {
        let (left, right, mesh_far, mesh_close, decoy, key_far, key_close, entity) = {
            let lab = self.curve_lab.as_ref().unwrap();
            (
                self.viewport_view.ok_or("Far view missing")?,
                lab.right_view.ok_or("Close view missing")?,
                lab.mesh_far.unwrap(),
                lab.mesh_close.unwrap(),
                lab.decoy.unwrap(),
                lab.key_far.clone(),
                lab.key_close.clone(),
                lab.solid.unwrap(),
            )
        };
        self.curve_bind_if_current(left, entity, mesh_close, &key_close, &key_far)?;
        self.curve_bind_if_current(right, entity, mesh_far, &key_far, &key_close)?;
        self.curve_bind_if_current(left, entity, decoy, "stale-decoy", &key_far)?;
        if self.renderer.as_ref().unwrap().resident_mesh_bytes(decoy).is_some() {
            return Err("the refused decoy was uploaded".into());
        }
        self.note_curve("Decoy mesh GPU_bytes: NOT UPLOADED");
        self.note_curve(&format!("stale uploads rejected: {}", self.curve_lab.as_ref().unwrap().discarded_stale_gpu));
        self.note_curve("Cross-view buffer substitution: NO");
        Ok(())
    }

    fn curve_bind_if_current(&mut self, view: RenderViewId, entity: jarvig_core::EntityId, mesh: MeshId, product_key: &str, current_key: &str) -> Result<(), String> {
        if product_key != current_key {
            self.curve_lab.as_mut().unwrap().discarded_stale_gpu = self.curve_lab.as_ref().unwrap().discarded_stale_gpu.saturating_add(1);
            return Ok(());
        }
        self.renderer.as_mut().unwrap().set_view_mesh_override(view, entity, Some(mesh)).map_err(|error| error.to_string())
    }

    fn curve_retire_mesh(&mut self, mesh: MeshId) -> Result<(), String> {
        let control = self.curve_lab.as_ref().unwrap().control_mesh.ok_or("control missing")?;
        let legacy = self.curve_lab.as_ref().unwrap().legacy_mesh.ok_or("legacy mesh missing")?;
        if mesh == control || mesh == legacy {
            return Err("refusing to retire the ordinary control mesh or the legacy cube".into());
        }
        if !self.engine.world_mut().retire_unreferenced_mesh(mesh) {
            return Err(format!("curve mesh {} stayed referenced", mesh.0));
        }
        self.curve_lab.as_mut().unwrap().retired.push(mesh);
        if self.engine.world().revision() != self.curve_lab.as_ref().unwrap().revision_before {
            return Err("retire_unreferenced_mesh revised the world".into());
        }
        Ok(())
    }
}

fn size_bits(size_m: [f64; 3]) -> String {
    format!("{:016x} {:016x} {:016x}", size_m[0].to_bits(), size_m[1].to_bits(), size_m[2].to_bits())
}

fn active_round(record: &jarvig_core::BlockRecord) -> Result<(String, f64), String> {
    let mut found = None;
    for entry in &record.intent {
        if let jarvig_core::IntentPayload::Round { radius_m } = entry.payload {
            let groups = entry.groups.as_ref().ok_or("a round has no semantic edge")?;
            if groups.len() != 1 || groups[0].len() != 1 || !groups[0][0].starts_with("E:") {
                return Err("a round does not name exactly one semantic edge".into());
            }
            found = Some((groups[0][0].clone(), radius_m));
        }
    }
    found.ok_or_else(|| "the tape has no round".into())
}

fn curve_key(product: &jarvig_core::RoundProduct) -> String {
    format!("curve7:{}:{:016x}:{:016x}", product.observation_id, product.radius_m.to_bits(), product.position_hash)
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
        } else if slice.contains("\"version\": 3") {
            versions.push(3);
        } else {
            return Err("a ParametricBlock version was not 1, 2, or 3".into());
        }
        rest = &rest[at + 24..];
    }
    if versions.is_empty() {
        return Err("the level file has no ParametricBlock".into());
    }
    Ok(versions)
}

fn curve_pose(solid: ResolvedPose, eye: [f64; 3], forward: [f64; 3]) -> Result<ResolvedPose, String> {
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
        return Err(format!("the view rotation does not look along the curve forward ({delta})"));
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

fn curve_wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn curve_rgb(red: u32, green: u32, blue: u32) -> COLORREF {
    red | (green << 8) | (blue << 16)
}

fn create_curve_hud(parent: HWND) -> Result<HWND, String> {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let class_name = curve_wide("JARVIG.CurveHud");
        let mut existing = WNDCLASSW { lpfnWndProc: Some(curve_hud_proc), hInstance: instance, lpszClassName: class_name.as_ptr(), ..std::mem::zeroed() };
        if GetClassInfoW(instance, class_name.as_ptr(), &mut existing) == 0 {
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(curve_hud_proc),
                hInstance: instance,
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszClassName: class_name.as_ptr(),
                ..std::mem::zeroed()
            };
            if RegisterClassW(&class) == 0 {
                return Err("the authored geometry hud class was not registered".into());
            }
        }
        let hwnd = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            curve_wide("AUTHORED GEOMETRY").as_ptr(),
            WS_POPUP | WS_VISIBLE | WS_CLIPSIBLINGS,
            0,
            0,
            640,
            196,
            parent,
            std::ptr::null_mut(),
            instance,
            std::ptr::null_mut(),
        );
        if hwnd.is_null() {
            Err(format!("the authored geometry hud was not created ({})", GetLastError()))
        } else {
            Ok(hwnd)
        }
    }
}

unsafe extern "system" fn curve_hud_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_NCHITTEST {
        return HTTRANSPARENT as LRESULT;
    }
    if message == WM_MOUSEACTIVATE {
        return 3;
    }
    if message == WM_ERASEBKGND {
        return 1;
    }
    if message != WM_PAINT && message != WM_PRINTCLIENT {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let mut paint: PAINTSTRUCT = std::mem::zeroed();
    let hdc = if message == WM_PAINT { BeginPaint(hwnd, &mut paint) } else { wparam as windows_sys::Win32::Graphics::Gdi::HDC };
    let mut client = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    GetClientRect(hwnd, &mut client);
    let brush = CreateSolidBrush(curve_rgb(16, 20, 28));
    FillRect(hdc, &client, brush);
    DeleteObject(brush as _);
    let font = GetStockObject(DEFAULT_GUI_FONT as i32);
    if !font.is_null() {
        SelectObject(hdc, font);
    }
    SetBkMode(hdc, TRANSPARENT as i32);
    SetTextColor(hdc, curve_rgb(236, 232, 214));
    let mut buffer = [0u16; 640];
    let len = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
    client.left += 12;
    client.top += 8;
    client.right -= 8;
    client.bottom -= 6;
    DrawTextW(hdc, buffer.as_ptr(), len, &mut client, DT_LEFT | DT_TOP | DT_NOPREFIX);
    if message == WM_PAINT {
        EndPaint(hwnd, &paint);
    }
    0
}
