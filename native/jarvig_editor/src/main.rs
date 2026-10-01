//! JARVIGEditor. A native shell around the engine, not a second engine.
//!
//! The dock tree is panel ids and ratios. Win32 handles are the realization.
//! The center panel is one engine `RenderView`. Panels do not draw the world.

mod camera;
mod chrome;
mod content_browser;
mod dock;
mod documents;
mod png_decode;
mod gizmo;
mod inspector;

mod outliner;
mod progress;
mod rfc_runtime;
mod selection;

use std::num::NonZeroIsize;
use std::time::Instant;

use dock::{
    dip_to_px, key_route, panel_takes_text, px_to_dip, Axis, DipPoint, DipRect, DockWorkspace, DropZone,
    Layout, PanelId, WorkspaceCommand, CONTENT, INSPECTOR, OUTLINER, OUTPUT, PERSPECTIVE, SCHEMA_VERSION,
};
use jarvig_core::{
    camera_relative_f32, environment_diffuse_for, environment_specular_for, perspective_ray, pick_snapshot_skipping, plus_z_normal, reflection_probe_influence,
    reflection_probe_lod, render_light_record, visible_side_normal, EntityHandle, EntityUuid, FocusError, LightKind, PropertyValue, Quat,
    RenderFrameId, ResolvedPose, ValueKind, Vec3, FIELD_LOCAL_ROTATION, REFLECTION_PROBE_MIP_COUNT, TYPE_CAMERA, TYPE_SPATIAL_FRAME,
};
use jarvig_engine::{AuthoringCommand, EngineSession, FrameError};
use jarvig_renderer::{
    EditorOverlay, FrameOutcome, LightingDebug, NormalizedRect, RenderError, RenderTargetId, RenderViewDesc, RenderViewId,
    PresentationMode, RenderViewSettings, RenderViewUpdate, Renderer, TerrainGridDesc,
};
use raw_window_handle::{DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, Win32WindowHandle, WindowHandle};
use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC, DeleteObject, EndPaint, ExcludeClipRect, FillRect,
    GetDC, GetDIBits, GetStockObject, GetTextExtentPoint32W, InvalidateRect, ReleaseDC, ScreenToClient, SelectObject, SetBkColor, SetBkMode,
    SetDIBitsToDevice, SetTextColor, TextOutW,
    UpdateWindow, BITMAPINFO, HBITMAP,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC, HFONT, PAINTSTRUCT, SRCCOPY, TRANSPARENT, DEFAULT_GUI_FONT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, GetKeyState, ReleaseCapture, SetCapture, VK_CONTROL, VK_ESCAPE, VK_MENU, VK_RETURN, VK_SHIFT,
};
use windows_sys::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_TREEVIEW_CLASSES, INITCOMMONCONTROLSEX, SetScrollInfo, NMTREEVIEWW, NMTVCUSTOMDRAW, NMTVGETINFOTIPW, CDDS_ITEMPREPAINT,
    CDDS_PREPAINT, CDRF_NEWFONT, CDRF_NOTIFYITEMDRAW, NM_CLICK, NM_CUSTOMDRAW, TVE_COLLAPSE, TVE_EXPAND, TVGN_CARET, TVGN_CHILD,
    TVGN_FIRSTVISIBLE, TVGN_NEXT, TVGN_ROOT, TVHT_ONITEM, TVIF_PARAM, TVIF_TEXT, TVI_LAST, TVI_ROOT, TVM_DELETEITEM, TVM_ENSUREVISIBLE,
    TVM_EXPAND, TVM_GETCOUNT, TVM_GETITEMW, TVM_GETNEXTITEM, TVM_HITTEST, TVM_INSERTITEMW, TVM_SELECTITEM, TVM_SETBKCOLOR,
    TVM_SETLINECOLOR, TVM_SETTEXTCOLOR, TVN_GETINFOTIPW, TVN_ITEMEXPANDEDW, TVN_KEYDOWN, TVN_SELCHANGEDW, TVS_DISABLEDRAGDROP, TVS_FULLROWSELECT,
    TVS_HASBUTTONS, TVS_HASLINES, TVS_INFOTIP, TVS_LINESATROOT, TVS_SHOWSELALWAYS, WC_TREEVIEWW, HTREEITEM, NMHDR, TVHITTESTINFO,
    TVINSERTSTRUCTW, TVITEMW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CheckMenuItem, CreateMenu, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW,
    GetMenuItemCount,
    CallWindowProcW, GetClientRect, GetCursorPos, GetMessageW, GetParent, GetWindowRect, GetWindowLongPtrW, GetWindowTextLengthW,
    GetWindowTextW, IsDialogMessageW, IsIconic, KillTimer, LoadCursorW,
    LoadIconW, MessageBoxW,
    PeekMessageW, PostMessageW, PostQuitMessage, RegisterClassW, SendMessageW, SetCursor, SetCursorPos, SetProcessDPIAware,
    SetTimer, SetWindowPos, SetWindowTextW, ShowCursor, ShowWindow, TrackPopupMenu, TranslateMessage, CREATESTRUCTW, CW_USEDEFAULT, GWLP_USERDATA, HMENU,
    SB_VERT, SCROLLINFO, SIF_PAGE, SIF_POS, SIF_RANGE,
    GWLP_WNDPROC, IDC_ARROW, IDC_SIZENS, IDC_SIZEWE, IDI_APPLICATION, MB_OK, MF_BYCOMMAND, MF_CHECKED, MF_POPUP, MF_SEPARATOR, MF_STRING,
    MF_UNCHECKED, MSG, PM_REMOVE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_MINIMIZE, SW_RESTORE, SW_SHOW, WM_CLOSE, WM_COMMAND, WM_CREATE,
    WM_ACTIVATE, WM_CAPTURECHANGED, WM_DESTROY, WM_DPICHANGED, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_NOTIFY, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SETCURSOR, WM_SIZE,
    WM_MOUSEACTIVATE, WM_MOVE, WM_TIMER, WNDCLASSW, WS_BORDER, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_VISIBLE, WS_VSCROLL,
};

const ID_FILE_NEW_PROJECT: usize = 1010;
const ID_FILE_OPEN_PROJECT: usize = 1011;
const ID_FILE_CLOSE_PROJECT: usize = 1012;
const ID_FILE_NEW_LEVEL: usize = 1013;
const ID_FILE_OPEN_LEVEL: usize = 1014;
const ID_FILE_SAVE: usize = 1015;
const ID_FILE_SAVE_AS: usize = 1016;
const ID_FILE_SAVE_ALL: usize = 1017;
const ID_FILE_RELOAD: usize = 1018;
const ID_FILE_IMPORT_MESH: usize = 1019;
const ID_EDIT_UNDO: usize = 1020;
const ID_FILE_RECENT_PROJECT: usize = 1040;
const ID_FILE_RECENT_LEVEL: usize = 1050;
const ID_FILE_TEMPLATE_EMPTY: usize = 1060;
const ID_FILE_TEMPLATE_TERRAIN: usize = 1061;
const ID_FILE_TEMPLATE_THIRD: usize = 1062;
const ID_FILE_TEMPLATE_FPS: usize = 1063;
const ID_FILE_EXIT: usize = 1001;
const ID_PLAY: usize = 1101;
const ID_PLAY_STANDALONE: usize = 1102;
const ID_BUILD_PROJECT: usize = 1401;
const ID_BUILD_AND_RUN: usize = 1402;
const ID_BUILD_SETTINGS: usize = 1403;
const ID_HELP_ABOUT: usize = 1201;
const ID_VIEW_OUTLINER: usize = 1301;
const ID_VIEW_CHARACTER: usize = 1390;
const ID_VIEW_RESET_POSE: usize = 1391;
const ID_VIEW_JOINTS: usize = 1392;
const ID_VIEW_JOINT_LIMITS: usize = 1393;
const ID_VIEW_ALL_JOINTS: usize = 1394;
const ID_VIEW_LAND: usize = 1395;
const ID_VIEW_INSPECTOR: usize = 1302;
const ID_VIEW_CONTENT: usize = 1303;
const ID_VIEW_OUTPUT: usize = 1304;
const ID_VIEW_RESET: usize = 1305;
const ID_VIEW_EXPOSURE_UP: usize = 1306;
const ID_VIEW_EXPOSURE_DOWN: usize = 1307;
const ID_VIEW_EXPOSURE_RESET: usize = 1308;
const ID_VIEW_ENVIRONMENT: usize = 1309;
const ID_DEBUG_FULL: usize = 1310;
const ID_DEBUG_DIRECT: usize = 1311;
const ID_DEBUG_ENV_DIFFUSE: usize = 1312;
const ID_DEBUG_ENV_SPECULAR: usize = 1313;
const ID_DEBUG_PROBE_ONLY: usize = 1314;
const ID_DEBUG_EMISSIVE: usize = 1315;
const ID_DEBUG_DIRECTIONAL: usize = 1316;
const ID_DEBUG_POINT: usize = 1317;
const ID_DEBUG_SPOT: usize = 1318;
const ID_DEBUG_GLOBAL_ENV: usize = 1319;
const ID_DEBUG_PROBE: usize = 1320;
const ID_DEBUG_DIRECTIONAL_ONLY: usize = 1321;
const ID_DEBUG_POINT_ONLY: usize = 1322;
const ID_DEBUG_SPOT_ONLY: usize = 1323;
const ID_DEBUG_NO_SHADOWS: usize = 1324;
const ID_DEBUG_INDIRECT_ONLY: usize = 1325;
const ID_DEBUG_DIRECT_UNSHADOWED: usize = 1326;
const ID_DEBUG_CASCADES: usize = 1337;
const ID_DEBUG_CONTACT: usize = 1338;
const ID_QUALITY_BASELINE: usize = 1339;
const ID_QUALITY_ENHANCED: usize = 1340;
const ID_QUALITY_HIGH: usize = 1341;
const ID_PRESENT_DITHER: usize = 1342;
const ID_PRESENT_TONEMAP: usize = 1343;
const ID_PRESENT_QUANTIZED: usize = 1344;
const ID_PRESENT_BEFORE: usize = 1345;
const ID_MAT_FULL: usize = 1346;
const ID_MAT_BASE: usize = 1347;
const ID_MAT_NORMAL: usize = 1348;
const ID_MAT_ROUGH: usize = 1349;
const ID_MAT_AO: usize = 1350;
const ID_MAT_METAL: usize = 1351;
const ID_VIEW_PILOT: usize = 1360;
const ID_VIEW_CREATE_CAMERA: usize = 1361;
const ID_VIEW_STARTUP_CAMERA: usize = 1362;
const ID_VIEW_MESHLETS: usize = 1363;
const ID_VIEW_MESHLET_SHADE: usize = 1364;
const ID_VIEW_MESHLET_FRUSTUM: usize = 1365;
const ID_VIEW_MESHLET_OCCLUSION: usize = 1366;
const ID_VIEW_MESHLET_FREEZE: usize = 1367;
const ID_VIEW_MESHLET_HIGHLIGHT: usize = 1368;
const ID_VIEW_CLUSTER_HIERARCHY: usize = 1369;
const ID_VIEW_BACKGROUND_JOBS: usize = 1370;
const ID_VIEW_CANCEL_JOB: usize = 1371;
const ID_VIEW_ERROR_HALF: usize = 1372;
const ID_VIEW_ERROR_ONE: usize = 1373;
const ID_VIEW_ERROR_TWO: usize = 1374;
const ID_VIEW_ERROR_FOUR: usize = 1375;
const ID_VIEW_EINSTEIN: usize = 1376;
const ID_VIEW_MICRO: usize = 1377;
const ID_VIEW_MICRO_COLOR: usize = 1378;
const ID_VIEW_RECAPTURE: usize = 1327;
const ID_PROBE_STATIC: usize = 1328;
const ID_PROBE_ON_DEMAND: usize = 1329;
const ID_PROBE_ON_TRANSFORM: usize = 1330;
const ID_PROBE_ON_LIGHTING: usize = 1331;
const ID_PROBE_RES_32: usize = 1332;
const ID_PROBE_RES_64: usize = 1333;
const ID_PROBE_RES_128: usize = 1334;
const ID_PROBE_RES_256: usize = 1335;
const ID_PROBE_TIME_SLICED: usize = 1336;
const TIMER_FRAME: usize = 1;
const WM_DOCK_TEST: u32 = 0x8001;
const WM_INSPECTOR_REFRESH: u32 = 0x8002;
const WM_PARENTNOTIFY: u32 = 0x0210;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_SETFONT: u32 = 0x0030;
const EN_SETFOCUS: u32 = 0x0100;
const EN_KILLFOCUS: u32 = 0x0200;
const ES_AUTOHSCROLL: u32 = 0x0080;
const WS_TABSTOP: u32 = 0x00010000;
const WM_KEYDOWN: u32 = 0x0100;
const WM_DRAWITEM: u32 = 0x002B;
const WM_MEASUREITEM: u32 = 0x002C;
const WM_MOUSELEAVE: u32 = 0x02A1;
const WM_CTLCOLOREDIT: u32 = 0x0133;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const LBN_SETFOCUS: u32 = 4;
const ES_MULTILINE: u32 = 0x0004;
const ES_READONLY: u32 = 0x0800;
const BS_AUTOCHECKBOX: u32 = 0x0003;
const CBS_DROPDOWNLIST: u32 = 0x0003;
const CBS_HASSTRINGS: u32 = 0x0200;
const WS_DISABLED: u32 = 0x0800_0000;
const CB_ADDSTRING: u32 = 0x0143;
const CB_SETCURSEL: u32 = 0x014E;
const CB_GETCURSEL: u32 = 0x0147;
const CB_GETLBTEXT: u32 = 0x0148;
const CB_SHOWDROPDOWN: u32 = 0x014F;
const BM_SETCHECK: u32 = 0x00F1;
const BM_GETCHECK: u32 = 0x00F0;
const CBN_SELCHANGE: u32 = 1;
const CBN_SETFOCUS: u32 = 3;
const WM_VSCROLL: u32 = 0x0115;
const WM_CTLCOLORBTN: u32 = 0x0135;
const WM_CTLCOLORLISTBOX: u32 = 0x0134;
const TPM_RETURNCMD: u32 = 0x0100;
const TPM_NONOTIFY: u32 = 0x0080;
const NM_SETFOCUS: u32 = 4294967289;
const SW_HIDE: i32 = 0;
const PW_RENDERFULLCONTENT: u32 = 0x0002;
const TOOLBAR_DIP: f32 = 40.0;
const STATUS_DIP: f32 = 22.0;
const CLASS_TOOLBAR: &str = "JARVIGEditorToolbar";
const CLASS_FRAME: &str = "JARVIGEditorFrame";
const CLASS_DOCK: &str = "JARVIGEditorDock";
const CLASS_VIEWPORT: &str = "JARVIGEditorViewport";
const CLASS_CONTENT: &str = "JARVIGEditorContent";
const CLASS_DROP: &str = "JARVIGEditorDrop";
const CLASS_INSPECTOR: &str = "JARVIGInspector";
const CLASS_LOAD: &str = "JARVIGLoadOverlay";
const CLASS_STATUS: &str = "JARVIGStatus";
const SHOT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../target/jarvig-editor-shell.bmp");
const SHOT_TABS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../target/jarvig-editor-tabs.bmp");
const SHOT_SELECTION: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../target/jarvig-editor-selection.bmp");
const SHOT_AUTHORING: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../target/jarvig-editor-authoring.bmp");
const SHOT_CAMERA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../target/jarvig-editor-camera.bmp");
const SHOT_ENVIRONMENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../target/jarvig-editor-environment.bmp");
const VK_A: usize = 0x41;
const VK_D: usize = 0x44;
const VK_DELETE: usize = 0x2E;

#[repr(C)]
struct OutlinerKey {
    _header: NMHDR,
    key: u16,
    _flags: u32,
}
const VK_E: usize = 0x45;
const VK_F: usize = 0x46;
const VK_Q: usize = 0x51;
const VK_S: usize = 0x53;
const VK_W: usize = 0x57;

#[repr(C)]
struct MouseTrack {
    size: u32,
    flags: u32,
    hwnd: HWND,
    time: u32,
}

#[link(name = "user32")]
extern "system" {
    fn PrintWindow(hwnd: HWND, hdc: HDC, flags: u32) -> i32;
    fn SetFocus(hwnd: HWND) -> HWND;
    fn TrackMouseEvent(event: *mut MouseTrack) -> i32;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Warmup,
    Resized,
    Split,
    Tabbed,
    Settling,
    Done,
}

/// Editor session over a runtime world. Not a menu string and not [`jarvig_core::ApplicationPhase`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum PlayPhase {
    Editing,
    StartingPlay,
    Playing,
    Paused,
    StoppingPlay,
}

enum Drag {
    Splitter { node: dock::NodeId },
    Tab { panel: PanelId, start: DipPoint, armed: bool },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CaptureKind {
    Look,
    Pan,
    Orbit,
    Gizmo,
    Terrain,
}

#[derive(Clone, Copy, Default)]
struct NavKeys {
    forward: bool,
    back: bool,
    left: bool,
    right: bool,
    down: bool,
    up: bool,
    boost: bool,
}

impl NavKeys {
    fn publish(self, nav: &mut camera::ViewportNavInput) {
        nav.forward = i32::from(self.forward) - i32::from(self.back);
        nav.strafe = i32::from(self.right) - i32::from(self.left);
        nav.vertical = i32::from(self.up) - i32::from(self.down);
        nav.boost = self.boost;
    }
}

struct RfcSample {
    threshold: f32,
    camera: String,
    distance: f64,
    depth: u32,
    leaves: u32,
    parents: u32,
    triangles: u32,
    original: u32,
    reduction: f32,
    select_us: u32,
    empty_parents: u32,
    root_triangles: u32,
    hole_ratio: f32,
    in_frame: bool,
    shop_height_px: f32,
    viewport_h: u32,
}

struct RfcReference {
    camera: String,
    rgba: Vec<u8>,
    width: i32,
    height: i32,
    origin_x: i32,
    origin_y: i32,
    view_w: i32,
    view_h: i32,
}

struct ShopTarget {
    center: Vec3,
    front: Vec3,
    corners: [Vec3; 8],
    half_x: f64,
    half_y: f64,
    half_z: f64,
}

struct ShopProjection {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    height: f64,
    in_frame: bool,
}

struct CapturedWindow {
    width: i32,
    height: i32,
    rgba: Vec<u8>,
}

struct EinsteinCapture {
    palette: f32,
    probe: jarvig_core::DetailProbe,
    draws: u32,
    triangles: u32,
    leaves: u32,
    projected_px: f32,
    rgba: Vec<u8>,
    width: i32,
    height: i32,
    region: (i32, i32, i32, i32),
}

struct MicroShot {
    name: String,
    base_triangles: u32,
    leaves: u32,
    leaf_triangles: u32,
    patches: u32,
    samples: u32,
    vertices: u32,
    triangles: u32,
    generation_us: u32,
    upload_us: u32,
    fallbacks: u32,
    ordinary: u32,
    fingerprint: u64,
    displacement_um: u32,
    projected_px: f32,
    orange: f32,
    rgba: Vec<u8>,
    width: i32,
    height: i32,
    region: (i32, i32, i32, i32),
}

struct SourceSeal {
    vertices: u32,
    bytes: u64,
    hash: u64,
    leaf_triangles: u32,
    hierarchy_nodes: u32,
    parents_len: u64,
    parents_modified: u64,
}

struct ReliefShot {
    rgba: Vec<u8>,
    width: i32,
    height: i32,
    region: (i32, i32, i32, i32),
    origin: (i32, i32, i32, i32),
    base_triangles: u32,
    micro_triangles: u32,
    vertices: u32,
    fingerprint: u64,
    invalid: u32,
    displacement_um: u32,
    projected_px: f32,
    generation_us: u32,
    upload_us: u32,
    reused: u32,
    leaf_triangles: u32,
}

struct TransitionRow {
    distance: f64,
    projected_px: f32,
    base_triangles: u32,
    micro_triangles: u32,
    vertices: u32,
    generation_us: u32,
    upload_us: u32,
    reused: u32,
    fingerprint: u64,
    invalid: u32,
    leaf_triangles: u32,
}

struct ParentJob {
    id: jarvig_core::JobId,
    mesh: jarvig_core::MeshId,
    last_percent: u32,
    last_state: jarvig_core::JobState,
    finished: bool,
}

struct MicroJob {
    id: jarvig_core::JobId,
    epoch: u64,
    finished: bool,
    cancel_noted: bool,
}

struct MicroJobProduct {
    epoch: u64,
    key: u64,
    mesh: jarvig_core::MicroMesh,
    queue_wait_us: u32,
    skipped: u32,
}

struct LandChunkProduct {
    epoch: u64,
    entity: jarvig_core::EntityId,
    revisions: Vec<(u32, u32, u64)>,
    built: Vec<jarvig_core::BuiltTerrainChunk>,
}

struct AsyncFrame {
    base: u32,
    leaf: u32,
    tris: u32,
    verts: u32,
    fingerprint: u64,
    partial: u32,
    stage: String,
}

struct Editor {
    engine: EngineSession,
    workspace: DockWorkspace,
    renderer: Option<Renderer>,
    target: Option<RenderTargetId>,
    viewport_view: Option<RenderViewId>,
    view_born: Option<RenderViewId>,
    viewport_born: HWND,
    frame: HWND,
    toolbar: HWND,
    dock_host: HWND,
    highlight: HWND,
    status: HWND,
    load_overlay: HWND,
    splash_pixels: Vec<u8>,
    splash_size: (i32, i32),
    splash_loaded: bool,
    /// Splash drawn once. Later paints blit this instead of rebuilding the image.
    splash_dc: HDC,
    splash_bitmap: HBITMAP,
    /// Splash popup rect in screen pixels. Zero means it still needs placing.
    load_placed: (i32, i32, i32, i32),
    progress: progress::Progress,
    /// Non-zero while a load is inside the UI thread. Nested frame ticks do not re-enter it.
    load_depth: u32,
    boot_failed: bool,
    panels: [(PanelId, HWND); 5],
    view_menu: HMENU,
    lighting_menu: HMENU,
    lighting_debug: LightingDebug,
    /// Session quality. Not saved in the level and not chosen from the adapter.
    render_quality: jarvig_core::RenderQuality,
    /// Capture face the quality level would author for a new probe. The open level keeps its own resolution.
    probe_budget: u32,
    cascade_debug: bool,
    contact_shadows: bool,
    /// 0 full shading. 1 base color, 2 normal, 3 roughness, 4 AO, 5 metallic. Not saved.
    material_channel: u32,
    output_dither: bool,
    presentation: PresentationMode,
    log: String,
    frames: u32,
    self_test: bool,
    limit: Option<u32>,
    boot_at: Instant,
    last: Instant,
    error: Option<String>,
    dpi: u32,
    last_layout: Layout,
    layout_needed: bool,
    drag: Option<Drag>,
    text_focused: bool,
    viewport_px: (u32, u32),
    born_viewport_px: (u32, u32),
    phase: Phase,
    posted: bool,
    reset_at: Option<u32>,
    outliner: outliner::WorldOutlinerModel,
    tree_keys: Vec<TreeKey>,
    tooltip: Vec<u16>,
    outliner_applying: bool,
    outliner_probed: bool,
    outliner_logged: bool,
    selection: selection::SelectionService,
    selection_seen: u64,
    selection_world_seen: u64,
    selection_view_ready: bool,
    pick_broadphase_us: u128,
    pick_mesh_us: u128,
    selection_change_us: u128,
    meshlet_debug: bool,
    meshlet_shade: bool,
    meshlet_frustum: bool,
    meshlet_occlusion: bool,
    meshlet_freeze: bool,
    meshlet_highlight: bool,
    cluster_hierarchy: bool,
    hierarchy_error_px: f32,
    einstein_debug: bool,
    einstein_seed: u64,
    einstein_uvs: Vec<[f32; 2]>,
    micro_enabled: bool,
    micro_surface: bool,
    micro_color: bool,
    micro_seed: u64,
    rfc0002: bool,
    rfc0002_micro: bool,
    micro_shots: Vec<MicroShot>,
    micro_seal: Option<SourceSeal>,
    rfc0002_transition: bool,
    rfc0002_refine: bool,
    rfc0002_async: bool,
    async_close: f64,
    async_far: f64,
    async_step: u32,
    async_busy: u32,
    async_frames: Vec<AsyncFrame>,
    async_fault: String,
    async_done: bool,
    transition_distances: Vec<f64>,
    transition_forward: usize,
    transition_index: usize,
    transition_rows: Vec<TransitionRow>,
    transition_off: Option<ReliefShot>,
    transition_on: Option<ReliefShot>,
    transition_color: Option<ReliefShot>,
    refine_hat: Option<ReliefShot>,
    refine_grazing: Option<ReliefShot>,
    transition_reasons: Vec<String>,
    transition_crops: Vec<(f32, Vec<u8>, i32, i32)>,
    transition_align: u8,
    rfc0002_off: Option<EinsteinCapture>,
    rfc0002_on: Option<EinsteinCapture>,
    rfc0002_again: Option<EinsteinCapture>,
    rfc0002_orbit: Option<EinsteinCapture>,
    rfc0002_far: Option<EinsteinCapture>,
    lod_capture: bool,
    rfc0001: bool,
    lod_phase: u8,
    lod_hold: u32,
    rfc_plan: Vec<(f32, String, f64)>,
    rfc_rows: Vec<RfcSample>,
    rfc_index: usize,
    rfc_cache_hit: bool,
    rfc_ref_dirty: bool,
    rfc_levels: Vec<jarvig_core::ParentLevelStats>,
    rfc_references: Vec<RfcReference>,
    rfc_runtime: bool,
    rfc_rebuild: bool,
    rfc_usable_ms: u128,
    rfc_dolly: Vec<rfc_runtime::RuntimeFrame>,
    rfc_distances: Vec<(String, f64)>,
    rfc_dolly_index: usize,
    rfc_checkpoints: Vec<(String, f64)>,
    rfc_checkpoint_index: usize,
    rfc_holes: Vec<rfc_runtime::HoleSample>,
    rfc_integrate: Vec<rfc_runtime::IntegrateSample>,
    jobs: jarvig_core::JobManager,
    parent_job: Option<ParentJob>,
    micro_job: Option<MicroJob>,
    registry_job: Option<jarvig_core::JobId>,
    browser: content_browser::BrowserModel,
    content_search: HWND,
    content_press: Option<(i32, i32, jarvig_core::AssetId)>,
    content_drag: Option<jarvig_core::AssetId>,
    content_undo: Vec<Vec<jarvig_core::EntityId>>,
    /// Character workspace over the same frames and joints. Not a second skeleton.
    character_workspace: bool,
    /// Land workspace over the same world. Not a second editor.
    land_mode: bool,
    outliner_land: bool,
    land_tool: LandTool,
    land_width: f32,
    land_depth: f32,
    land_spacing: f32,
    land_chunk: f32,
    land_height: f32,
    land_radius: f32,
    land_delta: f32,
    land_layer: u8,
    land_falloff: jarvig_core::TerrainFalloff,
    land_grid: bool,
    land_grid_minor: f32,
    land_grid_major: f32,
    land_grid_snap: bool,
    land_overlay_world: bool,
    land_overlay_vertices: bool,
    land_overlay_chunks: bool,
    land_overlay_lod: bool,
    land_show_terrain: bool,
    land_show_helpers: bool,
    land_show_lighting: bool,
    land_show_characters: bool,
    land_show_props: bool,
    land_show_gameplay: bool,
    land_show_full: bool,
    /// Last brush sample in terrain-local XZ. Debounces a drag.
    land_stamp: Option<(f32, f32)>,
    land_hover: Option<(f32, f32)>,
    land_flatten: Option<f32>,
    land_visual_epoch: u64,
    land_chunk_job: Option<jarvig_core::JobId>,
    land_chunk_rev: Vec<(u32, u32, u64)>,
    land_chunk_published: Vec<(u32, u32, u64)>,
    show_joint_debug: bool,
    show_joint_limits: bool,
    show_all_joints: bool,
    outliner_character: bool,
    drop_feedback: String,
    content_check: bool,
    bind_pose_dir: Option<std::path::PathBuf>,
    bind_pose_index: u8,
    bind_pose_capture_next: bool,
    job_line: String,
    meshlet_bound: Option<(jarvig_core::EntityId, bool)>,
    scene_meshes: Vec<jarvig_core::MeshId>,
    status_base: String,
    project_file: Option<std::path::PathBuf>,
    /// Set by `--project`. Cold start still opens Lighting Lab when this is empty.
    startup_project: Option<std::path::PathBuf>,
    project_name: String,
    level_file: Option<std::path::PathBuf>,
    level_name: String,
    level_uuid: Option<jarvig_core::EntityId>,
    saved_revision: u64,
    unsaved_policy: bool,
    recent_projects: Vec<std::path::PathBuf>,
    recent_levels: Vec<std::path::PathBuf>,
    last_autosave: Instant,
    recent_project_menu: HMENU,
    recent_level_menu: HMENU,
    inspector_model: inspector::InspectorModel,
    inspector_options: inspector::PlanOptions,
    inspector_controls: Vec<InspectorControl>,
    inspector_error: HWND,
    inspector_applying: bool,
    inspector_refresh_posted: bool,
    inspector_force_realize: bool,
    inspector_scroll: i32,
    inspector_span: i32,
    inspector_for: Option<EntityUuid>,
    inspector_collapsed: Vec<(EntityUuid, String)>,
    inspector_advanced_open: Vec<EntityUuid>,
    uniform_scale: bool,
    scrub_hwnd: HWND,
    scrub_x: i32,
    scrub_origin: f64,
    scrubbing: bool,
    authoring_restore: Option<(EntityUuid, jarvig_core::Vec3)>,
    authoring_pending_shot: bool,
    viewport_resize_requests: u32,
    viewport_zero_skips: u32,
    toolbar_icons: chrome::Toolbar,
    object_tool: chrome::ToolbarCommand,
    transform_space: gizmo::TransformSpace,
    gizmo_drag: Option<gizmo::GizmoDrag>,
    gizmo_hover: Option<gizmo::GizmoHandle>,
    pick_requests: u64,
    pick_hits: u64,
    pick_misses: u64,
    gizmo_updates: u64,
    gizmo_commits: u64,
    gizmo_cancels: u64,
    gizmo_restore: Option<(EntityUuid, Vec3, Quat)>,
    toolbar_hot: Option<usize>,
    ui_font: HFONT,
    font_dpi: u32,
    editor_camera: Option<camera::EditorCameraController>,
    /// Session preview of one game Camera. Not saved. Not the editor camera.
    pilot_entity: Option<EntityUuid>,
    play: PlayPhase,
    game: jarvig_core::GameApplication,
    play_control: jarvig_core::PlayControl,
    play_captured: bool,
    /// Editor camera pose saved at Play and restored at Stop. Not a scene object.
    play_camera: Option<ResolvedPose>,
    play_selection: Vec<selection::SelectionItem>,
    play_noted_missing_camera: bool,
    play_noted_ortho: bool,
    /// Distinguishes the authored outliner from the runtime one when revisions coincide.
    outliner_source: u64,
    inspector_source: u64,
    camera_home: Option<ResolvedPose>,
    nav: camera::ViewportNavInput,
    nav_keys: NavKeys,
    capture: Option<CaptureKind>,
    capture_point: POINT,
    capture_ending: bool,
    cursor_hidden: bool,
    camera_probe_pending: bool,
    camera_probed: bool,
    hdr_probe: u8,
    hdr_world: u64,
    hdr_selection: u64,
    hdr_compiles: u32,
    hdr_meshes: u32,
    hdr_textures: u32,
    hdr_pipelines: u32,
    env_probe: u8,
    env_rotation: Quat,
    env_base: [f32; 3],
    env_up: [f32; 3],
    env_compiles: u32,
    env_meshes: u32,
    env_textures: u32,
    env_tone: u32,
    env_uploads: u32,
    env_packets: u32,
    env_exposure_revision: u64,
    env_report: String,
    spec_probe: u8,
    spec_rotation: Quat,
    spec_camera: Vec3,
    spec_yaw: f64,
    spec_low: [f32; 3],
    spec_updates: u32,
    spec_revision: u64,
    spec_report: String,
    local_probe: u8,
    local_weight: f32,
    local_translation: Vec3,
    local_rotation: Quat,
    local_camera: Vec3,
    local_yaw: f64,
    local_compiles: u32,
    local_meshes: u32,
    local_textures: u32,
    local_captures: u32,
    local_revision: u64,
    local_report: String,
    camera_report: String,
}

/// Tree-control key. Lives only in the Win32 realization. Not the outliner model.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TreeKey {
    World,
    Entity(EntityUuid),
    Tool(LandTool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LandTool {
    Create,
    Select,
    Sculpt,
    Smooth,
    Flatten,
    Paint,
    Settings,
    Einstein,
    Debug,
}

impl LandTool {
    fn label(self) -> &'static str {
        match self {
            Self::Create => "Create Terrain",
            Self::Select => "Select Terrain",
            Self::Sculpt => "Sculpt",
            Self::Smooth => "Smooth",
            Self::Flatten => "Flatten",
            Self::Paint => "Paint Layers",
            Self::Settings => "Terrain Settings",
            Self::Einstein => "Einstein Detail",
            Self::Debug => "Debug",
        }
    }

    fn tip(self) -> &'static str {
        match self {
            Self::Create => "Flat heightfield from the inspector size.",
            Self::Select => "Select the terrain actor.",
            Self::Sculpt => "Raise the authoritative height. Shift lowers. Wheel changes radius.",
            Self::Smooth => "Average the authoritative height. Wheel changes radius.",
            Self::Flatten => "Pull height toward the brush center. Wheel changes radius.",
            Self::Paint => "Write a material class. Height stays put. Wheel changes radius.",
            Self::Settings => "Dimensions, collision, and LOD.",
            Self::Einstein => "Stored detail settings. They do not write height.",
            Self::Debug => "Height, slope, layer. Einstein detail is not generated.",
        }
    }

    const ALL: [LandTool; 9] = [
        Self::Create,
        Self::Select,
        Self::Sculpt,
        Self::Smooth,
        Self::Flatten,
        Self::Paint,
        Self::Settings,
        Self::Einstein,
        Self::Debug,
    ];

    fn key(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Select => "select",
            Self::Sculpt => "sculpt",
            Self::Smooth => "smooth",
            Self::Flatten => "flatten",
            Self::Paint => "paint",
            Self::Settings => "settings",
            Self::Einstein => "einstein",
            Self::Debug => "debug",
        }
    }

    fn from_key(text: &str) -> Option<Self> {
        Some(match text {
            "create" => Self::Create,
            "select" => Self::Select,
            "sculpt" => Self::Sculpt,
            "smooth" => Self::Smooth,
            "flatten" => Self::Flatten,
            "paint" => Self::Paint,
            "settings" => Self::Settings,
            "einstein" => Self::Einstein,
            "debug" => Self::Debug,
            _ => return None,
        })
    }
}

struct InspectorControl {
    hwnd: HWND,
    binding: inspector::InspectorBinding,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    stretch: bool,
    right_gutter: i32,
    right_slot: Option<i32>,
}

struct ViewportHandle(HWND);

impl HasWindowHandle for ViewportHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let hwnd = NonZeroIsize::new(self.0 as isize).ok_or(HandleError::NotSupported)?;
        let handle = Win32WindowHandle::new(hwnd);
        Ok(unsafe { WindowHandle::borrow_raw(handle.into()) })
    }
}

impl HasDisplayHandle for ViewportHandle {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::windows())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let self_test = args.iter().any(|arg| arg == "--self-test");
    let limit = args.iter().position(|arg| arg == "--frames").and_then(|index| {
        args.get(index + 1).and_then(|value| value.parse::<u32>().ok())
    });
    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) == 0 {
            let _ = SetProcessDPIAware();
        }
        let controls = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_TREEVIEW_CLASSES,
        };
        InitCommonControlsEx(&controls);
    }
    chrome::enable_dark_app();
    let mut editor = Editor::new(self_test, if self_test { None } else { limit });
    if args.iter().any(|arg| arg == "--lod-capture") {
        editor.lod_capture = true;
    }
    if args.iter().any(|arg| arg == "--rfc0001") {
        editor.lod_capture = true;
        editor.rfc0001 = true;
    }
    if args.iter().any(|arg| arg == "--rfc0001-runtime") {
        editor.lod_capture = true;
        editor.rfc_runtime = true;
    }
    if args.iter().any(|arg| arg == "--rfc0002") {
        editor.lod_capture = true;
        editor.rfc0002 = true;
    }
    if args.iter().any(|arg| arg == "--rfc0002-micro") {
        editor.lod_capture = true;
        editor.rfc0002_micro = true;
    }
    if args.iter().any(|arg| arg == "--rfc0002-transition") {
        editor.lod_capture = true;
        editor.rfc0002_transition = true;
    }
    if args.iter().any(|arg| arg == "--rfc0002-refine") {
        editor.lod_capture = true;
        editor.rfc0002_refine = true;
    }
    if args.iter().any(|arg| arg == "--rfc0002-async") {
        editor.lod_capture = true;
        editor.rfc0002_async = true;
    }
    if args.iter().any(|arg| arg == "--content-check") {
        editor.content_check = true;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--bind-pose-shots") {
        let Some(path) = args.get(index + 1) else {
            eprintln!("JARVIG_FAIL --bind-pose-shots needs a directory");
            std::process::exit(1);
        };
        editor.bind_pose_dir = Some(std::path::PathBuf::from(path));
    }
    if let Some(index) = args.iter().position(|arg| arg == "--project") {
        let Some(path) = args.get(index + 1) else {
            eprintln!("JARVIG_FAIL --project needs a .jarvigproject path");
            std::process::exit(1);
        };
        editor.startup_project = Some(std::path::PathBuf::from(path));
    }
    if let Err(error) = editor.run() {
        eprintln!("JARVIG_FAIL {error}");
        std::process::exit(1);
    }
    if editor.self_test && editor.frames < 6 {
        eprintln!("JARVIG_FAIL presented={}", editor.frames);
        std::process::exit(1);
    }
    println!("JARVIG_OK editor frames={}", editor.frames);
}

impl Editor {
    fn new(self_test: bool, limit: Option<u32>) -> Self {
        Self {
            engine: EngineSession::editor().expect("engine"),
            workspace: DockWorkspace::default_layout(),
            renderer: None,
            target: None,
            viewport_view: None,
            view_born: None,
            viewport_born: std::ptr::null_mut(),
            frame: std::ptr::null_mut(),
            toolbar: std::ptr::null_mut(),
            dock_host: std::ptr::null_mut(),
            highlight: std::ptr::null_mut(),
            status: std::ptr::null_mut(),
            load_overlay: std::ptr::null_mut(),
            splash_pixels: Vec::new(),
            splash_size: (0, 0),
            splash_loaded: false,
            splash_dc: std::ptr::null_mut(),
            splash_bitmap: std::ptr::null_mut(),
            load_placed: (0, 0, 0, 0),
            progress: progress::Progress::default(),
            load_depth: 0,
            boot_failed: false,
            panels: [(OUTLINER, std::ptr::null_mut()); 5],
            view_menu: std::ptr::null_mut(),
            lighting_menu: std::ptr::null_mut(),
            lighting_debug: LightingDebug::default(),
            render_quality: jarvig_core::RenderQuality::Baseline,
            probe_budget: jarvig_core::RenderQuality::Baseline.budget().probe_resolution,
            cascade_debug: false,
            contact_shadows: true,
            material_channel: 0,
            output_dither: true,
            presentation: PresentationMode::Tonemap,
            log: String::new(),
            frames: 0,
            self_test,
            limit,
            boot_at: Instant::now(),
            last: Instant::now(),
            error: None,
            dpi: 96,
            last_layout: Layout::default(),
            layout_needed: false,
            drag: None,
            text_focused: false,
            viewport_px: (0, 0),
            born_viewport_px: (0, 0),
            phase: Phase::Warmup,
            posted: false,
            reset_at: None,
            outliner: outliner::WorldOutlinerModel::empty(),
            tree_keys: Vec::new(),
            tooltip: Vec::new(),
            outliner_applying: false,
            outliner_probed: false,
            outliner_logged: false,
            selection: selection::SelectionService::default(),
            selection_seen: 0,
            selection_world_seen: 0,
            selection_view_ready: false,
            pick_broadphase_us: 0,
            pick_mesh_us: 0,
            selection_change_us: 0,
            meshlet_debug: false,
            meshlet_shade: false,
            meshlet_frustum: true,
            meshlet_occlusion: true,
            meshlet_freeze: false,
            meshlet_highlight: false,
            cluster_hierarchy: false,
            hierarchy_error_px: 1.0,
            einstein_debug: false,
            einstein_seed: 1,
            einstein_uvs: Vec::new(),
            micro_enabled: false,
            micro_surface: true,
            micro_color: false,
            micro_seed: 1,
            rfc0002: false,
            rfc0002_micro: false,
            micro_shots: Vec::new(),
            micro_seal: None,
            rfc0002_transition: false,
            rfc0002_refine: false,
            rfc0002_async: false,
            async_close: 0.0,
            async_far: 0.0,
            async_step: 0,
            async_busy: 0,
            async_frames: Vec::new(),
            async_fault: String::new(),
            async_done: false,
            transition_distances: Vec::new(),
            transition_forward: 0,
            transition_index: 0,
            transition_rows: Vec::new(),
            transition_off: None,
            transition_on: None,
            transition_color: None,
            refine_hat: None,
            refine_grazing: None,
            transition_reasons: Vec::new(),
            transition_crops: Vec::new(),
            transition_align: 0,
            rfc0002_off: None,
            rfc0002_on: None,
            rfc0002_again: None,
            rfc0002_orbit: None,
            rfc0002_far: None,
            lod_capture: false,
            rfc0001: false,
            lod_phase: 0,
            lod_hold: 0,
            rfc_plan: Vec::new(),
            rfc_rows: Vec::new(),
            rfc_index: 0,
            rfc_cache_hit: false,
            rfc_ref_dirty: false,
            rfc_levels: Vec::new(),
            rfc_references: Vec::new(),
            rfc_runtime: false,
            rfc_rebuild: false,
            rfc_usable_ms: 0,
            rfc_dolly: Vec::new(),
            rfc_distances: Vec::new(),
            rfc_dolly_index: 0,
            rfc_checkpoints: Vec::new(),
            rfc_checkpoint_index: 0,
            rfc_holes: Vec::new(),
            rfc_integrate: Vec::new(),
            jobs: jarvig_core::JobManager::new(2),
            parent_job: None,
            micro_job: None,
            registry_job: None,
            browser: content_browser::BrowserModel::default(),
            content_search: std::ptr::null_mut(),
            content_press: None,
            content_drag: None,
            content_undo: Vec::new(),
            character_workspace: false,
            land_mode: false,
            outliner_land: false,
            land_tool: LandTool::Create,
            land_width: jarvig_core::TERRAIN_DEFAULT_WIDTH_M,
            land_depth: jarvig_core::TERRAIN_DEFAULT_DEPTH_M,
            land_spacing: jarvig_core::TERRAIN_DEFAULT_SPACING_M,
            land_chunk: jarvig_core::TERRAIN_DEFAULT_CHUNK_M,
            land_height: jarvig_core::TERRAIN_DEFAULT_HEIGHT,
            land_radius: jarvig_core::TERRAIN_BRUSH_RADIUS_M,
            land_delta: jarvig_core::TERRAIN_BRUSH_DELTA_M,
            land_layer: 1,
            land_falloff: jarvig_core::TerrainFalloff::Smooth,
            land_grid: true,
            land_grid_minor: 1.0,
            land_grid_major: 10.0,
            land_grid_snap: false,
            land_overlay_world: true,
            land_overlay_vertices: false,
            land_overlay_chunks: false,
            land_overlay_lod: false,
            land_show_terrain: true,
            land_show_helpers: true,
            land_show_lighting: true,
            land_show_characters: false,
            land_show_props: false,
            land_show_gameplay: false,
            land_show_full: false,
            land_stamp: None,
            land_hover: None,
            land_flatten: None,
            land_visual_epoch: 1,
            land_chunk_job: None,
            land_chunk_rev: Vec::new(),
            land_chunk_published: Vec::new(),
            show_joint_debug: true,
            show_joint_limits: true,
            show_all_joints: false,
            outliner_character: false,
            drop_feedback: String::new(),
            content_check: false,
            bind_pose_dir: None,
            bind_pose_index: 0,
            bind_pose_capture_next: false,
            job_line: String::new(),
            meshlet_bound: None,
            scene_meshes: Vec::new(),
            status_base: "Starting".to_string(),
            project_file: None,
            startup_project: None,
            project_name: String::new(),
            level_file: None,
            level_name: String::new(),
            level_uuid: None,
            saved_revision: 0,
            unsaved_policy: false,
            recent_projects: Vec::new(),
            recent_levels: Vec::new(),
            last_autosave: Instant::now(),
            recent_project_menu: std::ptr::null_mut(),
            recent_level_menu: std::ptr::null_mut(),
            inspector_model: inspector::InspectorModel::empty(),
            inspector_options: inspector::PlanOptions::editing(),
            inspector_controls: Vec::new(),
            inspector_error: std::ptr::null_mut(),
            inspector_applying: false,
            inspector_refresh_posted: false,
            inspector_force_realize: false,
            inspector_scroll: 0,
            inspector_span: 0,
            inspector_for: None,
            inspector_collapsed: Vec::new(),
            inspector_advanced_open: Vec::new(),
            uniform_scale: false,
            scrub_hwnd: std::ptr::null_mut(),
            scrub_x: 0,
            scrub_origin: 0.0,
            scrubbing: false,
            authoring_restore: None,
            authoring_pending_shot: false,
            viewport_resize_requests: 0,
            viewport_zero_skips: 0,
            toolbar_icons: chrome::Toolbar::load(),
            object_tool: chrome::ToolbarCommand::Select,
            transform_space: gizmo::TransformSpace::World,
            gizmo_drag: None,
            gizmo_hover: None,
            pick_requests: 0,
            pick_hits: 0,
            pick_misses: 0,
            gizmo_updates: 0,
            gizmo_commits: 0,
            gizmo_cancels: 0,
            gizmo_restore: None,
            toolbar_hot: None,
            ui_font: std::ptr::null_mut(),
            font_dpi: 0,
            editor_camera: None,
            pilot_entity: None,
            play: PlayPhase::Editing,
            game: jarvig_core::GameApplication::new(),
            play_control: jarvig_core::PlayControl::inactive(),
            play_captured: false,
            play_camera: None,
            play_selection: Vec::new(),
            play_noted_missing_camera: false,
            play_noted_ortho: false,
            outliner_source: 0,
            inspector_source: 0,
            camera_home: None,
            nav: camera::ViewportNavInput::default(),
            nav_keys: NavKeys::default(),
            capture: None,
            capture_point: POINT { x: 0, y: 0 },
            capture_ending: false,
            cursor_hidden: false,
            camera_probe_pending: false,
            camera_probed: false,
            hdr_probe: 0,
            hdr_world: 0,
            hdr_selection: 0,
            hdr_compiles: 0u32,
            hdr_meshes: 0,
            hdr_textures: 0,
            hdr_pipelines: 0,
            env_probe: 0,
            env_rotation: Quat::IDENTITY,
            env_base: [0.0; 3],
            env_up: [0.0; 3],
            env_compiles: 0,
            env_meshes: 0,
            env_textures: 0,
            env_tone: 0,
            env_uploads: 0,
            env_packets: 0,
            env_exposure_revision: 0,
            env_report: String::new(),
            spec_probe: 0,
            spec_rotation: Quat::IDENTITY,
            spec_camera: Vec3::ZERO,
            spec_yaw: 0.0,
            spec_low: [0.0; 3],
            spec_updates: 0,
            spec_revision: 0,
            spec_report: String::new(),
            local_probe: 0,
            local_weight: 0.0,
            local_translation: Vec3::ZERO,
            local_rotation: Quat::IDENTITY,
            local_camera: Vec3::ZERO,
            local_yaw: 0.0,
            local_compiles: 0,
            local_meshes: 0,
            local_textures: 0,
            local_captures: 0,
            local_revision: 0,
            local_report: String::new(),
            camera_report: String::new(),
        }
    }

    fn run(&mut self) -> Result<(), String> {
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            register_classes(instance)?;
            let (menu, view_menu, lighting_menu, recent_projects, recent_levels) = editor_menu()?;
            self.view_menu = view_menu;
            self.lighting_menu = lighting_menu;
            self.recent_project_menu = recent_projects;
            self.recent_level_menu = recent_levels;
            let name = wide(CLASS_FRAME);
            let title = wide("JARVIGEditor");
            let hwnd = CreateWindowExW(
                0,
                name.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                1600,
                900,
                std::ptr::null_mut(),
                menu,
                instance,
                self as *mut Self as *const _,
            );
            if hwnd.is_null() {
                return Err("JARVIGEditor window was not created".into());
            }
            ShowWindow(hwnd, SW_SHOW);
            UpdateWindow(hwnd);
            let mut message: MSG = std::mem::zeroed();
            while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                let inspector = self.panel_hwnd(INSPECTOR);
                if inspector.is_null() || IsDialogMessageW(inspector, &message) == 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
        self.error.take().map_or(Ok(()), Err)
    }

    fn begin_load(&mut self) {
        self.load_depth = self.load_depth.saturating_add(1);
        if self.self_test {
            return;
        }
        self.progress.blocking = true;
        self.place_load_overlay();
    }

    fn end_load(&mut self) {
        self.load_depth = self.load_depth.saturating_sub(1);
    }

    /// Phase text is logged when it changes. `fraction` is `None` when the total is unknown.
    fn report(&mut self, phase: &str, detail: &str, fraction: Option<f32>) {
        if self.self_test {
            return;
        }
        let changed = self.progress.changed(phase, detail);
        self.progress.tick(phase, detail, fraction);
        self.progress.blocking = true;
        if changed {
            self.append(&self.progress.status_line());
        }
        self.push_progress_visuals();
    }

    fn fail_progress(&mut self, phase: &str, error: &str) {
        if self.self_test {
            return;
        }
        self.progress.fail(phase, error);
        self.append(&self.progress.status_line());
        self.push_progress_visuals();
    }

    fn push_progress_visuals(&mut self) {
        if self.status.is_null() {
            return;
        }
        self.set_status(&self.progress.status_line());
        self.place_load_overlay();
        self.invalidate_load_band();
    }

    /// Repaint the bar and the line under it. Never the whole splash.
    fn invalidate_load_band(&self) {
        if self.load_overlay.is_null() {
            return;
        }
        unsafe {
            let mut client: RECT = std::mem::zeroed();
            GetClientRect(self.load_overlay, &mut client);
            let bar = splash_bar_rect(self.splash_size, client.right, client.bottom);
            let band = RECT {
                left: 0,
                top: (bar.top - 4).max(0),
                right: client.right.max(1),
                bottom: (bar.bottom + 52).min(client.bottom.max(1)),
            };
            InvalidateRect(self.load_overlay, &band, 0);
        }
    }

    fn place_load_overlay(&mut self) {
        if self.self_test || self.load_overlay.is_null() || self.frame.is_null() {
            return;
        }
        if !self.progress.blocking {
            unsafe { ShowWindow(self.load_overlay, SW_HIDE); }
            self.load_placed = (0, 0, 0, 0);
            return;
        }
        let mut client: RECT = unsafe { std::mem::zeroed() };
        unsafe { GetClientRect(self.frame, &mut client); }
        let mut origin = POINT { x: 0, y: 0 };
        unsafe { ClientToScreen(self.frame, &mut origin); }
        let status = dip_to_px(STATUS_DIP, self.dpi.max(96)).max(1);
        let x = origin.x;
        let y = origin.y;
        let width = client.right.max(1);
        let height = (client.bottom - status).max(1);
        if self.load_placed == (x, y, width, height) {
            return;
        }
        unsafe {
            // Owned popup, above the swapchain. The view itself stays shown.
            SetWindowPos(
                self.load_overlay,
                std::ptr::null_mut(),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            self.load_placed = (x, y, width, height);
            InvalidateRect(self.load_overlay, std::ptr::null(), 0);
        }
    }

    fn hide_load_overlay(&mut self) {
        self.progress.clear();
        self.load_placed = (0, 0, 0, 0);
        if !self.load_overlay.is_null() {
            unsafe { ShowWindow(self.load_overlay, SW_HIDE); }
        }
        self.refresh_status();
    }

    /// Lets the window paint and answer Windows while a load step is still running.
    fn pump_loading(&mut self) {
        if self.self_test || self.frame.is_null() {
            return;
        }
        let animate = self.progress.blocking && self.progress.fraction.is_none() && self.progress.failed.is_none();
        self.progress.spinner = self.progress.spinner.wrapping_add(1);
        // A full invalidate erased the splash on every pump and strobed the window.
        if animate && self.progress.spinner % 5 == 0 {
            self.invalidate_load_band();
        }
        unsafe {
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                if message.message == 0x0012 {
                    PostQuitMessage(message.wParam as i32);
                    break;
                }
                if message.message == WM_TIMER {
                    continue;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    fn settle_load(&mut self) {
        if self.self_test || self.boot_failed || self.progress.failed.is_some() {
            return;
        }
        if !self.progress.blocking {
            self.note_probe_progress(false);
            return;
        }
        let (step, steps) = self.renderer.as_ref().map(|renderer| (renderer.probe_capture_job_step(), renderer.probe_capture_job_steps())).unwrap_or((0, 0));
        if self.frames == 0 {
            self.report("Uploading textures", "Waiting for the first presented frame.", None);
            return;
        }
        if steps > 0 {
            self.note_probe_progress(true);
            let _ = (step, steps);
            return;
        }
        self.append("Ready.");
        self.hide_load_overlay();
    }

    fn note_probe_progress(&mut self, cover: bool) {
        let Some(renderer) = self.renderer.as_ref() else { return };
        let steps = renderer.probe_capture_job_steps();
        if steps == 0 {
            return;
        }
        let step = renderer.probe_capture_job_step().min(steps);
        let fraction = Some(step as f32 / steps as f32);
        let detail = format!("{step} of {steps}");
        if cover {
            let changed = self.progress.changed("Capturing reflection probes", &detail);
            self.progress.tick("Capturing reflection probes", &detail, fraction);
            self.progress.blocking = true;
            if changed && step == 0 {
                self.append("Capturing reflection probes.");
            }
            self.push_progress_visuals();
        } else if self.progress.phase != "Capturing reflection probes" || self.progress.detail != detail {
            self.progress.tick("Capturing reflection probes", &detail, fraction);
            self.progress.blocking = false;
            self.refresh_status();
        }
    }

    fn boot_viewport(&mut self) -> Result<(), String> {
        self.begin_load();
        let result = self.boot_viewport_work();
        self.end_load();
        result
    }

    fn boot_viewport_work(&mut self) -> Result<(), String> {
        self.report("Starting editor", "The window is up.", None);
        self.pump_loading();
        if !self.self_test {
            self.boot_project()?;
        }
        self.realize();
        self.report("Initializing renderer", "Choosing an adapter.", None);
        self.pump_loading();
        let (width, height) = client_size(self.panel_hwnd(PERSPECTIVE));
        let attached = jarvig_rhi_wgpu::attach_window(
            &ViewportHandle(self.panel_hwnd(PERSPECTIVE)),
            width.max(1),
            height.max(1),
            &jarvig_rhi::GraphicsDeviceConfig::default(),
        )
        .map_err(|error| error.to_string())?;
        let graphics = format!(
            "Graphics {} — {} ({:?}, {}). {}",
            attached.selection.preference.label(),
            attached.selection.adapter.name,
            attached.selection.adapter.kind,
            attached.selection.adapter.api.label(),
            attached.selection.reason
        );
        println!("JARVIG {graphics}");
        self.append(&graphics);
        self.status_base = format!("Ready  |  {}  |  {:?}", attached.selection.adapter.name, attached.selection.adapter.kind);
        self.probe_budget = self.render_quality.budget().probe_resolution;
        self.append(&format!(
            "Renderer quality is {} on every adapter. It is not chosen from the vendor or from integrated versus discrete. Probe budget {} px. The open level keeps its authored capture. View > Probe Resolution compares 64, 128, and 256. Mip 0 stays the sharp capture.",
            self.render_quality.label(),
            self.probe_budget
        ));
        self.selection_view_ready = false;
        self.sync_selection_view();
        self.report("Building scene", "Creating the perspective view.", None);
        self.pump_loading();
        let mut renderer = Renderer::new(
            attached.device,
            attached.swapchain,
            attached.color_format,
            width.max(1),
            height.max(1),
        );
        let target = renderer.surface_target();
        let view = renderer
            .create_view(RenderViewDesc {
                label: "JARVIG.Perspective".into(),
                target,
                camera: self.engine.front_camera(),
                layout: NormalizedRect::FULL,
                settings: RenderViewSettings::default(),
            })
            .map_err(|error| error.to_string())?;
        self.target = Some(target);
        self.viewport_view = Some(view);
        self.view_born = Some(view);
        self.renderer = Some(renderer);
        self.viewport_px = (width.max(1), height.max(1));
        self.layout_needed = false;
        self.append(&format!(
            "World objects: {}. Entities: {}. Lights: {}. The viewport is one engine RenderView. Dock schema {}.",
            self.engine.world().object_count(),
            self.engine.world().entity_count(),
            self.engine.world_light_count(),
            self.workspace.schema_version()
        ));
        let snapshot = self.engine.world().extract(RenderFrameId(0)).map_err(|error| error.to_string())?;
        let front = self.engine.front_camera();
        let extracted = snapshot.camera(front.frame).ok_or("front camera missing from the snapshot")?;
        let controller = camera::EditorCameraController::from_pose(front.frame, extracted.pose, extracted.vertical_fov_radians, extracted.near_m);
        self.camera_home = Some(extracted.pose);
        self.editor_camera = Some(controller);
        self.push_editor_camera()?;
        self.restore_editor_viewport();
        let _ = self.push_editor_camera();
        self.append("Perspective camera is editor view state. It is not a scene entity, and flying it does not revise the world.");
        self.append("RMB looks. W/A/S/D flies along the look, including pitch. Q down, E up. Shift boosts without changing the saved speed. Wheel changes meters per second. MMB pans. Alt+LMB orbits. Alt+wheel dollies. F frames the selection.");
        self.append("Select picks a visible entity. Translate and Rotate drag an editor gizmo through engine commands. Scale stays disabled: a spatial frame stores translation and a quaternion, not scale.");
        self.append("With Perspective focused and no mouse capture, 1 selects, 2 translates, 3 rotates, and 4 reports that scale is unavailable. RMB capture still uses Q down and E up. W A S D still fly.");
        self.report("Uploading textures", "The first presented frame sends resident images.", None);
        self.pump_loading();
        if !self.toolbar_icons.load_note.is_empty() {
            self.append(&format!("Toolbar icons: {}", self.toolbar_icons.load_note));
        }
        self.sync_outliner();
        self.refresh_status();
        Ok(())
    }

    fn session_active(&self) -> bool {
        matches!(self.play, PlayPhase::Playing | PlayPhase::Paused)
    }

    fn viewed_world(&self) -> &jarvig_core::SceneWorld {
        if self.session_active() {
            if let Some(world) = self.game.runtime_world() {
                return world;
            }
        }
        self.engine.world()
    }

    fn view_source_key(&self) -> u64 {
        if self.session_active() { self.game.runtime_generation() } else { 0 }
    }

    fn sync_outliner(&mut self) {
        let source = self.view_source_key();
        let revision = self.viewed_world().revision();
        let land = self.land_mode && !self.session_active();
        let showing_tools = self.tree_keys.iter().any(|key| matches!(key, TreeKey::Tool(_)));
        if land {
            if self.outliner_land && self.outliner_source == source && !self.tree_keys.is_empty() {
                return;
            }
            self.outliner_land = true;
            self.outliner_character = false;
            self.outliner_source = source;
            self.realize_land_outliner();
            self.sync_selection_view();
            return;
        }
        if self.outliner.revision() == revision
            && self.outliner_source == source
            && self.outliner_character == self.character_workspace
            && !self.outliner_land
            && !showing_tools
            && !self.tree_keys.is_empty()
        {
            return;
        }
        let mut outline = self.viewed_world().entity_outline();
        if self.character_workspace {
            outline.retain(|row| self.viewed_world().authored_joint(row.uuid).is_some());
        }
        if !self.outliner_logged && self.project_file.is_some() {
            self.outliner_logged = true;
            self.append("Outliner reads the open level. Names are labels. Selection is a separate editor service.");
            for row in &outline {
                let name = outliner::presentation_name(&row.name);
                self.append(&format!("entity \"{name}\" {}", row.uuid));
            }
        }
        let next = outliner::WorldOutlinerModel::derive(revision, &outline, &self.outliner);
        // A pose or a simulation tick revises the world. The tree is the entity list, so those do not delete it.
        let same_rows = self.outliner_source == source
            && self.outliner_character == self.character_workspace
            && !self.outliner_land
            && !showing_tools
            && !self.tree_keys.is_empty()
            && self.outliner.same_rows(&next);
        self.outliner_source = source;
        self.outliner_character = self.character_workspace;
        self.outliner_land = false;
        self.outliner = next;
        if self.character_workspace {
            for row in &outline {
                if let Some(joint) = self.viewed_world().authored_joint(row.uuid) {
                    self.outliner.set_category(row.uuid, joint.kind.label());
                }
            }
        }
        if !same_rows {
            let live: Vec<EntityUuid> = outline.iter().map(|row| row.uuid).collect();
            self.selection.reconcile_entities(&live);
            self.realize_outliner();
        }
        self.sync_selection_view();
    }

    fn sync_selection_view(&mut self) {
        let world_revision = self.viewed_world().revision();
        let source = self.view_source_key();
        let selection_changed = !self.selection_view_ready || self.selection_seen != self.selection.revision() || self.inspector_source != source;
        let world_changed = self.selection_world_seen != world_revision;
        if !selection_changed && !world_changed {
            return;
        }
        self.selection_seen = self.selection.revision();
        self.selection_world_seen = world_revision;
        self.inspector_source = source;
        self.selection_view_ready = true;
        self.refresh_status();
        let inspector_started = Instant::now();
        self.rebuild_inspector();
        let inspector_us = inspector_started.elapsed().as_micros();
        if selection_changed && !self.self_test {
            self.append(&format!(
                "selection pick_broadphase_us={} pick_mesh_us={} selection_change_us={} inspector_refresh_us={} selection_highlight_us=0",
                self.pick_broadphase_us, self.pick_mesh_us, self.selection_change_us, inspector_us
            ));
        }
        if selection_changed {
            unsafe { InvalidateRect(self.panel_hwnd(OUTLINER), std::ptr::null(), 0); }
        }
    }

    fn rebuild_inspector(&mut self) {
        let land_focus = self.land_mode && !self.session_active();
        let entity = if land_focus { self.viewed_world().terrain_entity() } else { self.selection.primary_entity() };
        if self.inspector_for != entity {
            self.inspector_for = entity;
            self.inspector_scroll = 0;
        }
        let staged = if self.session_active() { Vec::new() } else { self.engine.staged_material_names() };
        let meshes = self.engine.mesh_asset_names();
        let mut model = if land_focus {
            if let Some(id) = entity {
                inspector::build_entity(self.viewed_world(), id, &staged, &meshes)
            } else {
                inspector::terrain_draft(
                    self.land_width as f64,
                    self.land_depth as f64,
                    self.land_spacing as f64,
                    self.land_chunk as f64,
                    self.land_height as f64,
                )
            }
        } else {
            let mut model = inspector::build_with(&self.selection, self.viewed_world(), &staged, &meshes);
            if let Some(entity) = self.selection.primary_entity() {
                inspector::attach_joint_context(&mut model, self.viewed_world(), entity);
            }
            model
        };
        if land_focus && entity.is_some() {
            inspector::attach_land_workspace(&mut model, self.land_inspector());
        }
        if self.session_active() {
            if let inspector::InspectorBody::Entity { sections } = &mut model.body {
                for section in sections {
                    for field in &mut section.fields {
                        field.editable = false;
                    }
                }
            }
        }
        if model == self.inspector_model && !self.inspector_controls.is_empty() && !self.inspector_force_realize {
            return;
        }
        let options = self.inspector_plan_options();
        let planned = inspector::plan(&model, &options);
        if self.inspector_can_patch(&options, &planned) {
            self.patch_inspector_controls(&planned);
            self.inspector_model = model;
            return;
        }
        self.inspector_force_realize = false;
        self.inspector_model = model;
        self.inspector_options = options;
        self.realize_inspector_controls();
    }

    fn inspector_can_patch(&self, options: &inspector::PlanOptions, planned: &[inspector::PlannedControl]) -> bool {
        if self.inspector_force_realize || self.inspector_controls.is_empty() || options != &self.inspector_options || self.inspector_controls.len() != planned.len() {
            return false;
        }
        let previous = inspector::plan(&self.inspector_model, options);
        previous.len() == planned.len() && previous.iter().zip(planned).all(|(left, right)| inspector::same_control_layout(left, right))
    }

    fn patch_inspector_controls(&mut self, planned: &[inspector::PlannedControl]) {
        self.inspector_applying = true;
        let focus = unsafe { GetFocus() };
        for (control, item) in self.inspector_controls.iter().zip(planned) {
            match item.class {
                inspector::ControlClass::Check => {
                    let want = usize::from(item.checked);
                    let have = unsafe { SendMessageW(control.hwnd, BM_GETCHECK, 0, 0) } as usize;
                    if have != want {
                        unsafe { SendMessageW(control.hwnd, BM_SETCHECK, want, 0); }
                    }
                }
                inspector::ControlClass::Combo => {
                    let have = unsafe { SendMessageW(control.hwnd, CB_GETCURSEL, 0, 0) } as usize;
                    if have != item.selected {
                        unsafe { SendMessageW(control.hwnd, CB_SETCURSEL, item.selected, 0); }
                    }
                }
                inspector::ControlClass::Edit => {
                    if item.enabled && control.hwnd == focus {
                        continue;
                    }
                    set_control_text(control.hwnd, &item.text);
                }
                inspector::ControlClass::Label | inspector::ControlClass::Button => set_control_text(control.hwnd, &item.text),
            }
        }
        self.inspector_applying = false;
    }

    fn request_inspector_refresh(&mut self) {
        if self.inspector_refresh_posted {
            return;
        }
        self.inspector_refresh_posted = true;
        unsafe { PostMessageW(self.panel_hwnd(INSPECTOR), WM_INSPECTOR_REFRESH, 0, 0); }
    }

    fn flush_inspector_refresh(&mut self) {
        self.inspector_refresh_posted = false;
        self.sync_outliner();
        self.selection_view_ready = false;
        self.sync_selection_view();
    }

    fn inspector_plan_options(&self) -> inspector::PlanOptions {
        let mut options = inspector::PlanOptions::editing();
        options.advanced_open = self.inspector_for.is_some_and(|id| self.inspector_advanced_open.contains(&id));
        options.collapsed = self
            .inspector_for
            .map(|id| self.inspector_collapsed.iter().filter(|(stored, _)| *stored == id).map(|(_, title)| title.clone()).collect())
            .unwrap_or_default();
        options.uniform_scale = self.uniform_scale;
        options.show_commands = !self.session_active();
        let terrain_selected = self.selection.primary_entity() == self.engine.world().terrain_entity() && self.engine.world().terrain_entity().is_some();
        options.show_add = !self.session_active() && self.selection.count() == 1 && (!self.land_mode || terrain_selected);
        if self.session_active() {
            options.banner = Some(
                if self.play == PlayPhase::Paused {
                    "Runtime world. Paused. Inspector is read only."
                } else {
                    "Runtime world. Inspector is read only."
                }
                .into(),
            );
        }
        options
    }

    fn realize_inspector_controls(&mut self) {
        let host = self.panel_hwnd(INSPECTOR);
        if host.is_null() {
            return;
        }
        self.inspector_applying = true;
        for control in self.inspector_controls.drain(..) {
            unsafe { DestroyWindow(control.hwnd); }
        }
        if !self.inspector_error.is_null() {
            unsafe { DestroyWindow(self.inspector_error); }
            self.inspector_error = std::ptr::null_mut();
        }
        self.inspector_options = self.inspector_plan_options();
        let planned = inspector::plan(&self.inspector_model, &self.inspector_options);
        for item in &planned {
            self.spawn_inspector_control(item);
        }
        self.layout_inspector_controls();
        self.inspector_applying = false;
    }

    fn spawn_inspector_control(&mut self, planned: &inspector::PlannedControl) {
        let host = self.panel_hwnd(INSPECTOR);
        let class_name = match planned.class {
            inspector::ControlClass::Label => "STATIC",
            inspector::ControlClass::Edit => "EDIT",
            inspector::ControlClass::Check | inspector::ControlClass::Button => "BUTTON",
            inspector::ControlClass::Combo => "COMBOBOX",
        };
        let mut style = match planned.class {
            inspector::ControlClass::Label => 0,
            inspector::ControlClass::Edit => WS_BORDER | ES_AUTOHSCROLL | if planned.enabled { WS_TABSTOP } else { ES_READONLY },
            inspector::ControlClass::Check => BS_AUTOCHECKBOX | WS_TABSTOP,
            inspector::ControlClass::Button => WS_TABSTOP,
            inspector::ControlClass::Combo => CBS_DROPDOWNLIST | CBS_HASSTRINGS | WS_VSCROLL | WS_TABSTOP,
        };
        if !planned.enabled && planned.class != inspector::ControlClass::Edit {
            style |= WS_DISABLED;
        }
        let hwnd = unsafe { create_inspector_child(host, class_name, &planned.text, style) };
        self.style_inspector_child(hwnd);
        chrome::use_dark_control(hwnd);
        if planned.class == inspector::ControlClass::Edit && planned.enabled {
            unsafe { subclass_inspector_edit(hwnd); }
        }
        if planned.class == inspector::ControlClass::Check {
            unsafe { SendMessageW(hwnd, BM_SETCHECK, if planned.checked { 1 } else { 0 }, 0); }
        }
        if planned.class == inspector::ControlClass::Combo {
            unsafe {
                for choice in &planned.choices {
                    let wide_choice = wide(choice);
                    SendMessageW(hwnd, CB_ADDSTRING, 0, wide_choice.as_ptr() as isize);
                }
                if !planned.choices.is_empty() {
                    SendMessageW(hwnd, CB_SETCURSEL, planned.selected, 0);
                }
            }
        }
        self.inspector_controls.push(InspectorControl {
            hwnd,
            binding: planned.binding.clone(),
            x: planned.x,
            y: planned.y,
            width: planned.width,
            height: planned.height,
            stretch: planned.stretch,
            right_gutter: planned.right_gutter,
            right_slot: planned.right_slot,
        });
    }

    fn style_inspector_child(&self, hwnd: HWND) {
        if hwnd.is_null() {
            return;
        }
        let font = if self.ui_font.is_null() { unsafe { GetStockObject(DEFAULT_GUI_FONT) as HFONT } } else { self.ui_font };
        unsafe { SendMessageW(hwnd, WM_SETFONT, font as WPARAM, 1); }
    }

    fn layout_inspector_controls(&mut self) {
        let host = self.panel_hwnd(INSPECTOR);
        let (width, height) = client_size(host);
        let width = width as i32;
        let height = height as i32;
        let mut content = 0i32;
        for control in &self.inspector_controls {
            content = content.max(control.y + control.height.max(20));
        }
        self.inspector_span = content + 16;
        let max_scroll = (self.inspector_span - height).max(0);
        self.inspector_scroll = self.inspector_scroll.clamp(0, max_scroll);
        for control in &self.inspector_controls {
            let (x, w) = if let Some(slot) = control.right_slot {
                let w = control.width.max(24);
                (width - 8 - (slot + 1) * (w + 4), w)
            } else if control.stretch {
                (control.x, (width - control.x - 8 - control.right_gutter).max(24))
            } else {
                (control.x, control.width.max(16))
            };
            place(control.hwnd, x, control.y - self.inspector_scroll, w, control.height);
        }
        self.sync_inspector_scroll(height);
    }

    fn sync_inspector_scroll(&self, height: i32) {
        let host = self.panel_hwnd(INSPECTOR);
        if host.is_null() {
            return;
        }
        unsafe {
            let mut info: SCROLLINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<SCROLLINFO>() as u32;
            info.fMask = SIF_RANGE | SIF_PAGE | SIF_POS;
            info.nMin = 0;
            info.nMax = self.inspector_span.max(0);
            info.nPage = height.max(1) as u32;
            info.nPos = self.inspector_scroll;
            SetScrollInfo(host, SB_VERT, &info, 1);
        }
    }

    fn on_inspector_scroll(&mut self, wparam: WPARAM) {
        let code = (wparam & 0xffff) as u16;
        let height = client_size(self.panel_hwnd(INSPECTOR)).1 as i32;
        let max_scroll = (self.inspector_span - height).max(0);
        let next = match code {
            0 => self.inspector_scroll - 48,
            1 => self.inspector_scroll + 48,
            2 => self.inspector_scroll - height.max(48),
            3 => self.inspector_scroll + height.max(48),
            4 | 5 => (wparam >> 16) as i16 as i32,
            6 => 0,
            7 => max_scroll,
            _ => return,
        };
        self.inspector_scroll = next.clamp(0, max_scroll);
        self.layout_inspector_controls();
    }

    fn on_inspector_wheel(&mut self, wparam: WPARAM) {
        let delta = ((wparam >> 16) as i16) as i32;
        let height = client_size(self.panel_hwnd(INSPECTOR)).1 as i32;
        let max_scroll = (self.inspector_span - height).max(0);
        self.inspector_scroll = (self.inspector_scroll - delta / 120 * 48).clamp(0, max_scroll);
        self.layout_inspector_controls();
    }

    fn commit_inspector_control(&mut self, hwnd: HWND) {
        if self.session_active() {
            self.append("Inspector is read only while playing. Stop returns to the authored world.");
            self.realize_inspector_controls();
            return;
        }
        if self.inspector_applying {
            return;
        }
        let Some(binding) = self.inspector_controls.iter().find(|control| control.hwnd == hwnd).map(|control| control.binding.clone()) else { return };
        if self.commit_land_ui(&binding, hwnd) {
            return;
        }
        if self.commit_land_draft(&binding, hwnd) {
            return;
        }
        let Some(entity) = self.inspector_target() else { return };
        match binding {
            inspector::InspectorBinding::Text { type_id, field } => self.commit_inspector_text(entity, type_id, field, &window_text(hwnd)),
            inspector::InspectorBinding::Axis { type_id, field, .. } => self.commit_inspector_axes(entity, type_id, field),
            _ => {}
        }
    }

    fn commit_inspector_text(&mut self, entity: EntityUuid, type_id: jarvig_core::TypeId, field: jarvig_core::FieldId, text: &str) {
        let Some(slot) = self.inspector_model.field(type_id, field).cloned() else { return };
        if !slot.editable {
            return;
        }
        let selection_revision = self.selection.revision();
        let submitted = if slot.kind == ValueKind::Float64 {
            let Some(value) = text.trim().parse::<f64>().ok().and_then(|value| inspector::clamp_number(value, slot.minimum, slot.maximum)) else {
                self.append("Inspector kept the authoritative value. The text was not a finite number.");
                self.realize_inspector_controls();
                return;
            };
            if slot.display.parse::<f64>().ok().is_some_and(|current| (current - value).abs() < 1.0e-9) {
                return;
            }
            inspector::submit_property(&mut self.engine, entity, type_id, field, PropertyValue::F64(value))
        } else if slot.kind == ValueKind::String {
            if text == slot.display {
                return;
            }
            inspector::submit_property(&mut self.engine, entity, type_id, field, PropertyValue::String(text.to_string()))
        } else {
            return;
        };
        self.finish_inspector_submit(selection_revision, &slot.display, field, submitted);
    }

    fn commit_inspector_axes(&mut self, entity: EntityUuid, type_id: jarvig_core::TypeId, field: jarvig_core::FieldId) {
        let Some(slot) = self.inspector_model.field(type_id, field).cloned() else { return };
        if !slot.editable {
            return;
        }
        let mut parts = [0.0; 3];
        for axis in 0..3 {
            let text = self.inspector_controls.iter().find_map(|control| match &control.binding {
                inspector::InspectorBinding::Axis { field: stored, axis: stored_axis, .. } if *stored == field && *stored_axis == axis => Some(window_text(control.hwnd)),
                _ => None,
            });
            let Some(value) = text.unwrap_or_default().trim().parse::<f64>().ok().filter(|value| value.is_finite()) else {
                self.append("Inspector kept the authoritative value. The text was not a finite number.");
                self.realize_inspector_controls();
                return;
            };
            parts[axis as usize] = inspector::clamp_number(value, slot.minimum, slot.maximum).unwrap_or(value);
        }
        if field == jarvig_core::FIELD_OBJECT_SCALE && self.uniform_scale {
            if let Some(axis) = (0..3).find(|axis| !axis_matches(&slot.components, *axis, parts[*axis as usize])) {
                let value = parts[axis as usize];
                parts = [value, value, value];
            }
        }
        if (0..3).all(|axis| axis_matches(&slot.components, axis, parts[axis as usize])) {
            return;
        }
        let selection_revision = self.selection.revision();
        let submitted = if slot.widget == inspector::WidgetKind::Euler {
            inspector::submit_property(
                &mut self.engine,
                entity,
                type_id,
                field,
                PropertyValue::Quat(inspector::quat_from_degrees(parts)),
            )
        } else {
            inspector::submit_property(&mut self.engine, entity, type_id, field, PropertyValue::Vec3(Vec3::new(parts[0], parts[1], parts[2])))
        };
        self.finish_inspector_submit(selection_revision, &slot.display, field, submitted);
    }

    fn commit_inspector_choice(&mut self, entity: EntityUuid, type_id: jarvig_core::TypeId, field: jarvig_core::FieldId, text: &str) {
        let Some(slot) = self.inspector_model.field(type_id, field).cloned() else { return };
        if !slot.editable {
            return;
        }
        let selection_revision = self.selection.revision();
        let submitted = if field == jarvig_core::FIELD_PARENT {
            match inspector::parent_from_choice(self.engine.world(), entity, text) {
                Ok(parent) => inspector::submit_property(&mut self.engine, entity, type_id, field, PropertyValue::OptionalEntity(parent)),
                Err(message) => {
                    self.append(&format!("Inspector kept the authoritative value. {message}."));
                    self.realize_inspector_controls();
                    return;
                }
            }
        } else if slot.kind == ValueKind::Float64 {
            let Some(value) = inspector::choice_as_number(text) else {
                self.realize_inspector_controls();
                return;
            };
            if slot.display.parse::<f64>().ok().is_some_and(|current| (current - value).abs() < 1.0e-9) {
                return;
            }
            inspector::submit_property(&mut self.engine, entity, type_id, field, PropertyValue::F64(value))
        } else if text == slot.display {
            return;
        } else {
            inspector::submit_property(&mut self.engine, entity, type_id, field, PropertyValue::String(text.to_string()))
        };
        self.finish_inspector_submit(selection_revision, &slot.display, field, submitted);
    }

    fn commit_inspector_check(&mut self, entity: EntityUuid, type_id: jarvig_core::TypeId, field: jarvig_core::FieldId, checked: bool) {
        let Some(slot) = self.inspector_model.field(type_id, field).cloned() else { return };
        if !slot.editable {
            self.realize_inspector_controls();
            return;
        }
        let current = matches!(slot.display.as_str(), "true" | "1");
        if current == checked {
            return;
        }
        let selection_revision = self.selection.revision();
        let submitted = inspector::submit_property(&mut self.engine, entity, type_id, field, PropertyValue::Bool(checked));
        self.finish_inspector_submit(selection_revision, &slot.display, field, submitted);
    }

    fn finish_inspector_submit(
        &mut self,
        selection_revision: u64,
        display: &str,
        field: jarvig_core::FieldId,
        submitted: Result<jarvig_core::AuthoringResult, jarvig_core::AuthoringError>,
    ) {
        if self.selection.revision() != selection_revision {
            self.append("Inspector edit changed selection. That is a bug.");
        }
        match submitted {
            Ok(_) => {
                if display.to_ascii_lowercase().contains("emissive") {
                    self.engine.world_mut().note_lighting_edit();
                }
                if field == jarvig_core::FIELD_UPDATE_MODE {
                    self.note_policy_edit();
                }
                self.request_inspector_refresh();
            }
            Err(error) => {
                self.append(&format!("Inspector command failed: {error}"));
                self.inspector_force_realize = true;
                self.request_inspector_refresh();
            }
        }
    }

    fn on_inspector_command(&mut self, hwnd: HWND) {
        if self.inspector_applying {
            return;
        }
        let Some(binding) = self.inspector_controls.iter().find(|control| control.hwnd == hwnd).map(|control| control.binding.clone()) else { return };
        if self.session_active() {
            if matches!(binding, inspector::InspectorBinding::Section { .. }) {
                if let inspector::InspectorBinding::Section { title } = binding {
                    self.toggle_inspector_section(&title);
                }
                return;
            }
            self.append("Inspector is read only while playing. Stop returns to the authored world.");
            self.realize_inspector_controls();
            return;
        }
        match binding {
            inspector::InspectorBinding::Section { title } => self.toggle_inspector_section(&title),
            inspector::InspectorBinding::Check { field, .. } if inspector::is_land_ui(field) => {
                let checked = unsafe { SendMessageW(hwnd, BM_GETCHECK, 0, 0) } == 1;
                self.set_land_check(field, checked);
            }
            inspector::InspectorBinding::Check { type_id, field } => {
                let checked = unsafe { SendMessageW(hwnd, BM_GETCHECK, 0, 0) } == 1;
                let Some(entity) = self.inspector_target() else { return };
                self.commit_inspector_check(entity, type_id, field, checked);
            }
            inspector::InspectorBinding::Choice { field, .. } if inspector::is_land_ui(field) => {
                self.set_land_choice(field, &combo_text(hwnd));
            }
            inspector::InspectorBinding::Choice { type_id, field } => {
                let Some(entity) = self.inspector_target() else { return };
                self.commit_inspector_choice(entity, type_id, field, &combo_text(hwnd));
            }
            inspector::InspectorBinding::Command { command, type_id } => self.run_inspector_command(command, type_id),
            inspector::InspectorBinding::AddComponent => self.open_add_component_menu(),
            inspector::InspectorBinding::Browse { type_id, field } => self.open_inspector_choice(type_id, field),
            inspector::InspectorBinding::Color { type_id, field } => self.pick_inspector_color(type_id, field),
            inspector::InspectorBinding::Uniform => {
                self.uniform_scale = unsafe { SendMessageW(hwnd, BM_GETCHECK, 0, 0) } == 1;
            }
            _ => {}
        }
    }

    fn toggle_inspector_section(&mut self, title: &str) {
        let Some(entity) = self.inspector_for else { return };
        if title == "Advanced" {
            if let Some(index) = self.inspector_advanced_open.iter().position(|id| *id == entity) {
                self.inspector_advanced_open.remove(index);
            } else {
                self.inspector_advanced_open.push(entity);
            }
        } else if let Some(index) = self.inspector_collapsed.iter().position(|(id, name)| *id == entity && name == title) {
            self.inspector_collapsed.remove(index);
        } else {
            self.inspector_collapsed.push((entity, title.to_string()));
        }
        self.realize_inspector_controls();
    }

    fn run_inspector_command(&mut self, command: inspector::InspectorCommand, type_id: jarvig_core::TypeId) {
        if command == inspector::InspectorCommand::CreateTerrain {
            self.create_land_terrain();
            return;
        }
        let Some(entity) = self.inspector_target() else { return };
        match command {
            inspector::InspectorCommand::RemoveComponent => {
                match self.engine.execute_authoring(AuthoringCommand::RemoveComponent { target: entity, type_id, slot: 0 }) {
                    Ok(_) => self.sync_outliner(),
                    Err(jarvig_core::AuthoringError::InvalidOperation) => {
                        self.append("That component is required by another one. Remove the dependent component first.");
                    }
                    Err(error) => self.append(&format!("Remove component failed: {error}")),
                }
            }
            inspector::InspectorCommand::ResetTransform => self.reset_authored_transform(entity),
            inspector::InspectorCommand::ResetMaterial => self.reset_material_instance(entity),
            inspector::InspectorCommand::SetStartupCamera => self.set_startup_camera(),
            inspector::InspectorCommand::Recapture => {
                self.engine.world_mut().request_probe_recapture();
                self.append("Reflection probes marked for recapture. The camera was not moved.");
                self.refresh_status();
            }
            inspector::InspectorCommand::ResetPose => self.reset_character_pose(),
            inspector::InspectorCommand::CreateTerrain => self.create_land_terrain(),
        }
    }

    fn reset_authored_transform(&mut self, entity: EntityUuid) {
        if let Some(joint) = self.engine.world().authored_joint(entity) {
            let _ = inspector::submit_property(
                &mut self.engine,
                entity,
                jarvig_core::TYPE_SPATIAL_FRAME,
                jarvig_core::FIELD_LOCAL_TRANSLATION,
                PropertyValue::Vec3(joint.rest_translation),
            );
            let _ = inspector::submit_property(
                &mut self.engine,
                entity,
                jarvig_core::TYPE_SPATIAL_FRAME,
                jarvig_core::FIELD_LOCAL_ROTATION,
                PropertyValue::Quat(joint.rest_rotation),
            );
            self.append("Joint returned to its rest pose.");
            return;
        }
        let _ = inspector::submit_property(
            &mut self.engine,
            entity,
            jarvig_core::TYPE_SPATIAL_FRAME,
            jarvig_core::FIELD_LOCAL_TRANSLATION,
            PropertyValue::Vec3(Vec3::new(0.0, 0.0, 0.0)),
        );
        let _ = inspector::submit_property(
            &mut self.engine,
            entity,
            jarvig_core::TYPE_SPATIAL_FRAME,
            jarvig_core::FIELD_LOCAL_ROTATION,
            PropertyValue::Quat(Quat::IDENTITY),
        );
        if self.inspector_model.field(jarvig_core::TYPE_MESH_RENDERER, jarvig_core::FIELD_OBJECT_SCALE).is_some() {
            let _ = inspector::submit_property(
                &mut self.engine,
                entity,
                jarvig_core::TYPE_MESH_RENDERER,
                jarvig_core::FIELD_OBJECT_SCALE,
                PropertyValue::Vec3(Vec3::new(1.0, 1.0, 1.0)),
            );
        }
        self.sync_outliner();
    }

    fn reset_material_instance(&mut self, entity: EntityUuid) {
        for field in [jarvig_core::FIELD_UV_SCALE, jarvig_core::FIELD_ROUGHNESS_FACTOR, jarvig_core::FIELD_METALLIC_FACTOR, jarvig_core::FIELD_NORMAL_SCALE] {
            let _ = inspector::submit_property(&mut self.engine, entity, jarvig_core::TYPE_MESH_RENDERER, field, PropertyValue::F64(1.0));
        }
        self.sync_outliner();
    }

    fn open_inspector_choice(&mut self, type_id: jarvig_core::TypeId, field: jarvig_core::FieldId) {
        let Some(hwnd) = self.inspector_controls.iter().find_map(|control| match &control.binding {
            inspector::InspectorBinding::Choice { type_id: stored_type, field: stored } if *stored_type == type_id && *stored == field => Some(control.hwnd),
            _ => None,
        }) else {
            return;
        };
        unsafe { SendMessageW(hwnd, CB_SHOWDROPDOWN, 1, 0); }
    }

    fn pick_inspector_color(&mut self, type_id: jarvig_core::TypeId, field: jarvig_core::FieldId) {
        if self.session_active() {
            return;
        }
        let Some(entity) = self.selection.primary_entity() else { return };
        let Some(slot) = self.inspector_model.field(type_id, field).cloned() else { return };
        if !slot.editable {
            return;
        }
        let mut parts = [1.0, 1.0, 1.0];
        for (index, text) in slot.components.iter().take(3).enumerate() {
            if let Ok(value) = text.parse::<f64>() {
                parts[index] = value;
            }
        }
        let Some(color) = choose_linear_color(self.panel_hwnd(INSPECTOR), Vec3::new(parts[0], parts[1], parts[2])) else { return };
        let selection_revision = self.selection.revision();
        let submitted = inspector::submit_property(&mut self.engine, entity, type_id, field, PropertyValue::Vec3(color));
        self.finish_inspector_submit(selection_revision, &slot.display, field, submitted);
    }

    fn open_add_component_menu(&mut self) {
        if self.session_active() {
            return;
        }
        let Some(entity) = self.selection.primary_entity() else { return };
        let owned = inspector::owned_types(self.engine.world(), entity);
        let choices = inspector::add_component_choices(&owned);
        if choices.is_empty() {
            self.append("Every component this actor can take is already on it.");
            return;
        }
        let picked = unsafe {
            let menu = CreatePopupMenu();
            for (index, (_, name)) in choices.iter().enumerate() {
                let label = wide(name);
                AppendMenuW(menu, MF_STRING, index + 1, label.as_ptr());
            }
            let mut point = POINT { x: 0, y: 0 };
            GetCursorPos(&mut point);
            let picked = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY, point.x, point.y, 0, self.frame, std::ptr::null());
            DestroyMenu(menu);
            picked
        };
        if picked == 0 {
            return;
        }
        let Some((type_id, name)) = choices.get((picked as usize).saturating_sub(1)).copied() else { return };
        self.add_inspector_component(entity, type_id, name);
    }

    fn add_inspector_component(&mut self, entity: EntityUuid, type_id: jarvig_core::TypeId, name: &str) {
        let requires = jarvig_core::find_type(type_id).map(|info| info.requires.to_vec()).unwrap_or_default();
        let owned = inspector::owned_types(self.engine.world(), entity);
        for required in requires {
            if owned.contains(&required) {
                continue;
            }
            let required_name = jarvig_core::find_type(required).map(|info| info.display_name).unwrap_or("component");
            match self.engine.execute_authoring(AuthoringCommand::AddComponent { target: entity, type_id: required }) {
                Ok(_) => self.append(&format!("{required_name} was added because {name} needs it.")),
                Err(error) => {
                    self.append(&format!("{name} needs {required_name}, and that could not be added ({error})."));
                    return;
                }
            }
        }
        match self.engine.execute_authoring(AuthoringCommand::AddComponent { target: entity, type_id }) {
            Ok(_) => {
                self.append(&format!("{name} added."));
                self.sync_outliner();
            }
            Err(jarvig_core::AuthoringError::InvalidOperation) => {
                self.append(&format!("{name} is already on this actor, or this actor cannot take it."));
            }
            Err(jarvig_core::AuthoringError::Unsupported) => {
                self.append(&format!("{name} cannot be added from the inspector."));
            }
            Err(error) => self.append(&format!("Could not add {name}: {error}")),
        }
    }

    fn step_inspector_edit(&mut self, hwnd: HWND, direction: f64) {
        if self.session_active() || self.inspector_applying {
            return;
        }
        let Some(binding) = self.inspector_controls.iter().find(|control| control.hwnd == hwnd).map(|control| control.binding.clone()) else { return };
        let (type_id, field) = match binding {
            inspector::InspectorBinding::Text { type_id, field } | inspector::InspectorBinding::Axis { type_id, field, .. } => (type_id, field),
            _ => return,
        };
        let Some(slot) = self.inspector_model.field(type_id, field) else { return };
        if !slot.editable || slot.step == 0.0 {
            return;
        }
        let current = window_text(hwnd).trim().parse::<f64>().unwrap_or(0.0);
        let Some(next) = inspector::clamp_number(current + direction * slot.step, slot.minimum, slot.maximum) else { return };
        let precision = slot.precision.min(6) as usize;
        let text = format!("{next:.precision$}");
        let wide_text = wide(&text);
        unsafe { SetWindowTextW(hwnd, wide_text.as_ptr()); }
        self.commit_inspector_control(hwnd);
    }

    fn scrub_inspector_edit(&mut self, hwnd: HWND, x: i32, starting: bool) {
        if self.session_active() {
            return;
        }
        if starting {
            self.scrub_hwnd = hwnd;
            self.scrub_x = x;
            self.scrub_origin = window_text(hwnd).trim().parse::<f64>().unwrap_or(0.0);
            self.scrubbing = false;
            return;
        }
        if self.scrub_hwnd != hwnd {
            return;
        }
        let dx = x - self.scrub_x;
        if !self.scrubbing {
            if dx.abs() < 6 {
                return;
            }
            self.scrubbing = true;
            unsafe { SetCapture(hwnd); }
        }
        let Some(binding) = self.inspector_controls.iter().find(|control| control.hwnd == hwnd).map(|control| control.binding.clone()) else { return };
        let (type_id, field) = match binding {
            inspector::InspectorBinding::Text { type_id, field } | inspector::InspectorBinding::Axis { type_id, field, .. } => (type_id, field),
            _ => return,
        };
        let Some(slot) = self.inspector_model.field(type_id, field) else { return };
        if !slot.editable || slot.step == 0.0 {
            return;
        }
        let next = inspector::clamp_number(self.scrub_origin + dx as f64 * slot.step, slot.minimum, slot.maximum);
        let Some(next) = next else { return };
        let precision = slot.precision.min(6) as usize;
        let text = format!("{next:.precision$}");
        let wide_text = wide(&text);
        self.inspector_applying = true;
        unsafe { SetWindowTextW(hwnd, wide_text.as_ptr()); }
        self.inspector_applying = false;
    }

    fn revert_inspector_control(&mut self, _hwnd: HWND) {
        self.realize_inspector_controls();
    }

    fn apply_outliner_activation(&mut self, row: Option<EntityUuid>, ctrl: bool) -> Result<(), String> {
        let caret = match row {
            None => outliner::OutlinerNodeId::WorldRoot,
            Some(id) => outliner::OutlinerNodeId::Entity(id),
        };
        self.outliner.set_caret(Some(caret));
        self.pick_broadphase_us = 0;
        self.pick_mesh_us = 0;
        let started = Instant::now();
        selection::activate_outliner_row(&mut self.selection, row, ctrl).map_err(|error| error.to_string())?;
        self.selection_change_us = started.elapsed().as_micros();
        self.sync_selection_view();
        Ok(())
    }

    fn on_outliner_click(&mut self) {
        let hwnd = self.panel_hwnd(OUTLINER);
        unsafe {
            let mut point = POINT { x: 0, y: 0 };
            if GetCursorPos(&mut point) == 0 || ScreenToClient(hwnd, &mut point) == 0 {
                return;
            }
            let mut hit: TVHITTESTINFO = std::mem::zeroed();
            hit.pt = point;
            let item = SendMessageW(hwnd, TVM_HITTEST, 0, &mut hit as *mut TVHITTESTINFO as LPARAM);
            if item == 0 || (hit.flags & TVHT_ONITEM) == 0 {
                return;
            }
            let mut read: TVITEMW = std::mem::zeroed();
            read.mask = TVIF_PARAM;
            read.hItem = item;
            if SendMessageW(hwnd, TVM_GETITEMW, 0, &mut read as *mut TVITEMW as LPARAM) == 0 {
                return;
            }
            let Some(key) = self.tree_key(read.lParam) else {
                return;
            };
            let ctrl = GetKeyState(VK_CONTROL as i32) < 0;
            match key {
                TreeKey::Tool(tool) => {
                    self.land_tool = tool;
                    match tool {
                        LandTool::Select => self.select_terrain_actor(),
                        LandTool::Debug => self.log_terrain_debug(),
                        _ => {
                            self.inspector_force_realize = true;
                            self.request_inspector_refresh();
                        }
                    }
                    self.persist_workspace();
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
                TreeKey::World => {
                    let _ = self.apply_outliner_activation(None, ctrl);
                }
                TreeKey::Entity(id) => {
                    let _ = self.apply_outliner_activation(Some(id), ctrl);
                }
            }
        }
    }

    fn paint_outliner_row(&self, lparam: LPARAM) -> LRESULT {
        let draw = unsafe { &mut *(lparam as *mut NMTVCUSTOMDRAW) };
        match draw.nmcd.dwDrawStage {
            CDDS_PREPAINT => CDRF_NOTIFYITEMDRAW as LRESULT,
            CDDS_ITEMPREPAINT => {
                let mut text = chrome::TEXT;
                let mut background = chrome::PANEL;
                match self.tree_key(draw.nmcd.lItemlParam) {
                    Some(TreeKey::Entity(id)) => {
                        let item = selection::SelectionItem::Entity(id);
                        if self.selection.contains(item) {
                            text = chrome::TEXT;
                            background = if self.selection.primary() == Some(item) { chrome::SELECT } else { chrome::SELECT_SOFT };
                        }
                    }
                    Some(TreeKey::Tool(tool)) if tool == self.land_tool => {
                        text = chrome::TEXT;
                        background = chrome::SELECT;
                    }
                    _ => {}
                }
                draw.clrText = text;
                draw.clrTextBk = background;
                CDRF_NEWFONT as LRESULT
            }
            _ => 0,
        }
    }

    fn realize_outliner(&mut self) {
        let hwnd = self.panel_hwnd(OUTLINER);
        if hwnd.is_null() {
            return;
        }
        let keep_scroll = self.first_visible_key();
        let caret = self.outliner.caret();
        self.outliner_applying = true;
        self.tree_keys.clear();
        unsafe { SendMessageW(hwnd, TVM_DELETEITEM, 0, TVI_ROOT); }
        let root = if self.session_active() {
            "Runtime"
        } else if self.character_workspace {
            "Skeleton"
        } else {
            "World"
        };
        let world_item = self.insert_tree_item(TVI_ROOT, root, TreeKey::World);
        let roots = self.outliner.roots().to_vec();
        self.insert_entity_rows(world_item, &roots);
        if self.outliner.world_expanded() && world_item != 0 {
            unsafe { SendMessageW(hwnd, TVM_EXPAND, TVE_EXPAND as usize, world_item); }
        }
        if let Some(item) = self.item_for(caret) {
            unsafe { SendMessageW(hwnd, TVM_SELECTITEM, TVGN_CARET as usize, item); }
        }
        if let Some(item) = keep_scroll.and_then(|key| self.item_for(Some(key_to_node(key)))) {
            unsafe { SendMessageW(hwnd, TVM_ENSUREVISIBLE, 0, item); }
        }
        self.outliner_applying = false;
    }

    fn insert_entity_rows(&mut self, parent: HTREEITEM, entities: &[EntityUuid]) {
        let entities = entities.to_vec();
        for entity in entities {
            let name = self.outliner.row_text(entity).unwrap_or_else(|| "Unnamed".to_string());
            let children = self.outliner.children(entity).to_vec();
            let expanded = self.outliner.is_expanded(entity);
            let item = self.insert_tree_item(parent, &name, TreeKey::Entity(entity));
            self.insert_entity_rows(item, &children);
            if expanded && item != 0 && !children.is_empty() {
                let hwnd = self.panel_hwnd(OUTLINER);
                unsafe { SendMessageW(hwnd, TVM_EXPAND, TVE_EXPAND as usize, item); }
            }
        }
    }

    fn insert_tree_item(&mut self, parent: HTREEITEM, text: &str, key: TreeKey) -> HTREEITEM {
        let hwnd = self.panel_hwnd(OUTLINER);
        let param = self.tree_keys.len() as isize;
        self.tree_keys.push(key);
        let mut label = wide(text);
        let item = unsafe {
            let mut insert: TVINSERTSTRUCTW = std::mem::zeroed();
            insert.hParent = parent;
            insert.hInsertAfter = TVI_LAST;
            insert.Anonymous.item = TVITEMW {
                mask: TVIF_TEXT | TVIF_PARAM,
                hItem: 0,
                state: 0,
                stateMask: 0,
                pszText: label.as_mut_ptr(),
                cchTextMax: label.len() as i32,
                iImage: 0,
                iSelectedImage: 0,
                cChildren: 0,
                lParam: param,
            };
            SendMessageW(hwnd, TVM_INSERTITEMW, 0, &mut insert as *mut TVINSERTSTRUCTW as LPARAM)
        };
        item
    }

    fn first_visible_key(&self) -> Option<TreeKey> {
        let hwnd = self.panel_hwnd(OUTLINER);
        if hwnd.is_null() || self.tree_keys.is_empty() {
            return None;
        }
        unsafe {
            let item = SendMessageW(hwnd, TVM_GETNEXTITEM, TVGN_FIRSTVISIBLE as usize, 0);
            if item == 0 {
                return None;
            }
            let mut read: TVITEMW = std::mem::zeroed();
            read.mask = TVIF_PARAM;
            read.hItem = item;
            if SendMessageW(hwnd, TVM_GETITEMW, 0, &mut read as *mut TVITEMW as LPARAM) == 0 {
                return None;
            }
            self.tree_key(read.lParam)
        }
    }

    fn tree_key(&self, param: LPARAM) -> Option<TreeKey> {
        self.tree_keys.get(param as usize).copied()
    }

    fn item_for(&self, node: Option<outliner::OutlinerNodeId>) -> Option<HTREEITEM> {
        let node = node?;
        let index = self.tree_keys.iter().position(|key| match (node, key) {
            (outliner::OutlinerNodeId::WorldRoot, TreeKey::World) => true,
            (outliner::OutlinerNodeId::Entity(entity), TreeKey::Entity(stored)) => entity == *stored,
            _ => false,
        })?;
        let item = self.find_item_by_param(index as isize);
        if item == 0 { None } else { Some(item) }
    }

    fn find_item_by_param(&self, param: isize) -> HTREEITEM {
        let hwnd = self.panel_hwnd(OUTLINER);
        unsafe { find_item(hwnd, TVI_ROOT, param).unwrap_or(0) }
    }

    fn on_outliner_notify(&mut self, lparam: LPARAM) -> LRESULT {
        let header = unsafe { &*(lparam as *const NMHDR) };
        if header.hwndFrom != self.panel_hwnd(OUTLINER) {
            return 0;
        }
        if header.code == NM_CUSTOMDRAW {
            return self.paint_outliner_row(lparam);
        }
        if self.outliner_applying {
            return 0;
        }
        match header.code {
            NM_SETFOCUS => self.note_child_focus(OUTLINER.raw()),
            NM_CLICK => self.on_outliner_click(),
            TVN_KEYDOWN => {
                let key = unsafe { &*(lparam as *const OutlinerKey) };
                let ctrl = unsafe { GetKeyState(VK_CONTROL as i32) } < 0;
                if key.key as usize == VK_DELETE {
                    self.destroy_selected();
                } else if ctrl && key.key as usize == VK_D {
                    self.duplicate_selected();
                }
            }
            TVN_SELCHANGEDW => {
                let view = unsafe { &*(lparam as *const NMTREEVIEWW) };
                if let Some(key) = self.tree_key(view.itemNew.lParam) {
                    self.outliner.set_caret(Some(key_to_node(key)));
                }
            }
            TVN_ITEMEXPANDEDW => {
                let view = unsafe { &*(lparam as *const NMTREEVIEWW) };
                let expanded = view.action != TVE_COLLAPSE;
                match self.tree_key(view.itemNew.lParam) {
                    Some(TreeKey::World) => self.outliner.set_world_expanded(expanded),
                    Some(TreeKey::Entity(entity)) => self.outliner.set_expanded(entity, expanded),
                    Some(TreeKey::Tool(_)) | None => {}
                }
            }
            TVN_GETINFOTIPW => {
                let tip = unsafe { &mut *(lparam as *mut NMTVGETINFOTIPW) };
                let text = match self.tree_key(tip.lParam) {
                    Some(TreeKey::World) => "Editor root. Not an entity.".to_string(),
                    Some(TreeKey::Entity(entity)) => entity.to_string(),
                    Some(TreeKey::Tool(tool)) => tool.tip().to_string(),
                    None => return 0,
                };
                self.tooltip = wide(&text);
                tip.pszText = self.tooltip.as_mut_ptr();
            }
            _ => {}
        }
        0
    }

    fn probe_outliner(&mut self) -> Result<(), String> {
        let layout = self.workspace.persistent();
        let focus = self.workspace.focused();
        self.sync_outliner();
        let world = self.engine.world();
        let outline = world.entity_outline();
        if outline.len() != 7 || outline[0].name != "Near Triangle" || outline[1].name != "Far Triangle" {
            return Err(format!("bootstrap outliner {:?}", outline.iter().map(|row| row.name.clone()).collect::<Vec<_>>()));
        }
        if outline.iter().any(|row| row.name.starts_with("Object ") || row.uuid.to_string().len() != 36) {
            return Err("bootstrap row used an object slot as its label".into());
        }
        let near = outline[0].uuid;
        let far = outline[1].uuid;
        if self.outliner.display_name(near) != Some("Near Triangle") || self.outliner.roots().len() != 7 || self.outliner.roots()[0] != near || self.outliner.roots()[1] != far {
            return Err("outliner model did not follow registry order".into());
        }
        let marker: EntityHandle = self.engine.world_mut().create_entity("Empty Entity");
        let marker_id = self.engine.world().resolve(marker).map_err(|error| error.to_string())?;
        self.sync_outliner();
        if self.engine.world().entity_count() != 8 || self.outliner.entity_count() != 8 {
            return Err("empty entity did not appear in the outliner".into());
        }
        if self.outliner.display_name(marker_id) != Some("Empty Entity")
            || self.outliner.node(marker_id).and_then(|node| node.parent).is_some()
        {
            return Err("empty entity parent or name was wrong".into());
        }
        let snapshot = self.engine.world().extract(RenderFrameId(1)).map_err(|error| error.to_string())?;
        if snapshot.instance_count() != 2 || snapshot.instances().iter().any(|instance| instance.entity == marker_id) {
            return Err("outliner membership followed the render snapshot".into());
        }
        let probed_entities = self.engine.world().entity_count();
        let probed_instances = snapshot.instance_count();
        let probed_nodes = self.outliner.entity_count();
        let probed_roots = self.outliner.root_entity_count();
        let tree_count = unsafe { SendMessageW(self.panel_hwnd(OUTLINER), TVM_GETCOUNT, 0, 0) };
        if tree_count != 9 {
            return Err(format!("tree count {tree_count}, expected world plus eight entities"));
        }
        self.engine.world_mut().rename_entity(marker, "Gameplay Marker").map_err(|error| error.to_string())?;
        self.sync_outliner();
        if self.engine.world().resolve(marker).ok() != Some(marker_id) || self.outliner.display_name(marker_id) != Some("Gameplay Marker") {
            return Err("rename changed the uuid or skipped the label".into());
        }
        let copy = self.engine.world_mut().duplicate_entity(marker).map_err(|error| error.to_string())?;
        let copy_id = self.engine.world().resolve(copy).map_err(|error| error.to_string())?;
        if copy_id == marker_id {
            return Err("duplicate reused the uuid".into());
        }
        self.engine.world_mut().reparent_entity(copy, Some(marker)).map_err(|error| error.to_string())?;
        self.sync_outliner();
        if self.outliner.node(copy_id).map(|node| node.parent) != Some(Some(marker_id))
            || self.outliner.children(marker_id) != [copy_id].as_slice()
        {
            return Err("reparent did not update the outliner".into());
        }
        if self.engine.world_mut().reparent_entity(marker, Some(copy)).is_ok() {
            return Err("outliner allowed a parent cycle".into());
        }
        if self.outliner.node(marker_id).map(|node| node.parent) != Some(None) {
            return Err("rejected cycle still moved the parent".into());
        }
        let retired = self.engine.world_mut().retire_entity(copy).map_err(|error| error.to_string())?;
        if retired != copy_id || self.engine.world().resolve(copy).is_ok() {
            return Err("retired handle still resolved".into());
        }
        self.sync_outliner();
        if self.outliner.node(copy_id).is_some() {
            return Err("retired uuid stayed in the outliner".into());
        }
        let reused = self.engine.world_mut().create_entity("Reuse Check");
        if reused.index != copy.index || reused.generation == copy.generation || self.engine.world().resolve(copy).is_ok() {
            return Err("reused slot resurrected the retired handle".into());
        }
        let reused_id = self.engine.world().resolve(reused).map_err(|error| error.to_string())?;
        self.sync_outliner();
        if self.outliner.node(copy_id).is_some() || self.outliner.display_name(reused_id) != Some("Reuse Check") {
            return Err("reused slot resurrected the retired uuid".into());
        }
        println!(
            "JRV-0061 probe entity_count={probed_entities} render_instances={probed_instances} outliner_nodes={probed_nodes} outliner_roots={probed_roots} rename=PASS duplicate=PASS reparent=PASS cycle=PASS retire=PASS stale=PASS"
        );
        self.engine.world_mut().retire_entity(reused).map_err(|error| error.to_string())?;
        self.engine.world_mut().retire_entity(marker).map_err(|error| error.to_string())?;
        self.sync_outliner();
        if self.engine.world().entity_count() != 7 || self.engine.world().object_count() != 2 || self.outliner.entity_count() != 7 {
            return Err("outliner probe did not restore the bootstrap world".into());
        }
        if self.workspace.persistent() != layout || self.workspace.focused() != focus {
            return Err("outliner refresh changed the dock workspace".into());
        }
        if self.panel_hwnd(PERSPECTIVE) != self.viewport_born || self.viewport_view != self.view_born {
            return Err("outliner refresh changed the viewport".into());
        }
        if unsafe { SendMessageW(self.panel_hwnd(OUTLINER), TVM_GETCOUNT, 0, 0) } != 8 {
            return Err("restored tree did not contain world plus the seven authorable rows".into());
        }
        self.probe_selection()
    }

    fn probe_selection(&mut self) -> Result<(), String> {
        let layout = self.workspace.persistent();
        let compiles = self.engine.material_compile_count();
        let world_before = self.engine.world().revision();
        if !self.selection.is_empty() {
            return Err("selection was not empty after bootstrap".into());
        }
        let outline = self.engine.world().entity_outline();
        let near = outline[0].uuid;
        let far = outline[1].uuid;
        let near_handle = self.engine.world().find_entity(near).map_err(|error| error.to_string())?;
        let far_handle = self.engine.world().find_entity(far).map_err(|error| error.to_string())?;
        self.outliner.set_caret(Some(outliner::OutlinerNodeId::WorldRoot));
        self.selection.replace(selection::SelectionItem::entity(near).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
        self.sync_selection_view();
        if self.selection.primary_entity() != Some(near) || self.outliner.caret() != Some(outliner::OutlinerNodeId::WorldRoot) {
            return Err("programmatic selection followed the outliner caret".into());
        }
        self.apply_outliner_activation(Some(near), false)?;
        if self.selection.primary_entity() != Some(near) || !self.inspector_contains("Near Triangle") || !self.inspector_contains("NEAR TRIANGLE") {
            return Err("inspector did not follow near selection".into());
        }
        if self.inspector_contains(&near.to_string()) {
            return Err("the entity uuid was drawn on the inspector face".into());
        }
        if !window_text(self.status).contains("Selected: Near Triangle") {
            return Err("status bar did not observe the selection".into());
        }
        let _ = capture_window(self.frame, SHOT_SELECTION);
        let revision_after_click = self.selection.revision();
        self.apply_outliner_activation(Some(far), true)?;
        if self.selection.count() != 2 || self.selection.primary_entity() != Some(far) || !self.selection.contains(selection::SelectionItem::Entity(near)) {
            return Err("ctrl click did not add the second entity".into());
        }
        if !self.inspector_contains("2 entities selected") {
            return Err("inspector did not summarize multi-selection".into());
        }
        self.workspace.apply(WorkspaceCommand::Focus(OUTPUT)).map_err(|error| error.to_string())?;
        if self.workspace.focused() != Some(OUTPUT) || self.selection.count() != 2 {
            return Err("moving focus cleared selection".into());
        }
        if self.selection.revision() != revision_after_click.saturating_add(1) {
            return Err("focus changed the selection revision".into());
        }
        self.workspace.apply(WorkspaceCommand::ClosePanel(OUTLINER)).map_err(|error| error.to_string())?;
        self.workspace.apply(WorkspaceCommand::ShowPanel(OUTLINER)).map_err(|error| error.to_string())?;
        self.realize();
        self.workspace.apply(WorkspaceCommand::ClosePanel(INSPECTOR)).map_err(|error| error.to_string())?;
        self.workspace.apply(WorkspaceCommand::ShowPanel(INSPECTOR)).map_err(|error| error.to_string())?;
        self.realize();
        self.sync_selection_view();
        if self.selection.count() != 2 || !self.inspector_contains("2 entities selected") {
            return Err("hiding a panel dropped selection".into());
        }
        self.workspace.apply(WorkspaceCommand::ResetLayout).map_err(|error| error.to_string())?;
        self.realize();
        if self.selection.count() != 2 || self.selection.primary_entity() != Some(far) || self.workspace.persistent() != layout {
            return Err("reset layout changed selection".into());
        }
        self.workspace.apply(WorkspaceCommand::Activate(OUTPUT)).map_err(|error| error.to_string())?;
        if self.selection.primary_entity() != Some(far) {
            return Err("activating the output tab changed selection".into());
        }
        self.workspace.apply(WorkspaceCommand::Focus(PERSPECTIVE)).map_err(|error| error.to_string())?;
        if self.engine.world().revision() != world_before || self.engine.material_compile_count() != compiles {
            return Err("selection mutated the world or the material pipeline".into());
        }
        self.engine.world_mut().rename_entity(near_handle, "Player Start").map_err(|error| error.to_string())?;
        self.sync_outliner();
        if self.selection.primary_entity() != Some(far) || !self.selection.contains(selection::SelectionItem::Entity(near)) {
            return Err("rename changed selection identity".into());
        }
        self.selection.replace(selection::SelectionItem::entity(near).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
        self.sync_selection_view();
        if !self.inspector_contains("Player Start") || !self.inspector_contains("PLAYER START") {
            return Err("rename did not refresh the inspector summary".into());
        }
        self.engine.world_mut().rename_entity(near_handle, "Near Triangle").map_err(|error| error.to_string())?;
        self.apply_outliner_activation(Some(far), false)?;
        self.engine.world_mut().reparent_entity(far_handle, Some(near_handle)).map_err(|error| error.to_string())?;
        self.sync_outliner();
        if self.selection.primary_entity() != Some(far) || self.outliner.node(far).and_then(|node| node.parent) != Some(near) {
            return Err("reparent changed selection or skipped the tree".into());
        }
        self.engine.world_mut().reparent_entity(far_handle, None).map_err(|error| error.to_string())?;
        let duplicate = self.engine.world_mut().duplicate_entity(far_handle).map_err(|error| error.to_string())?;
        let duplicate_id = self.engine.world().resolve(duplicate).map_err(|error| error.to_string())?;
        self.sync_outliner();
        if self.selection.primary_entity() != Some(far) || self.selection.contains(selection::SelectionItem::Entity(duplicate_id)) {
            return Err("duplicate became selected".into());
        }
        self.engine.world_mut().retire_entity(duplicate).map_err(|error| error.to_string())?;
        let alpha = self.engine.world_mut().create_entity("Alpha");
        let beta = self.engine.world_mut().create_entity("Beta");
        let gamma = self.engine.world_mut().create_entity("Gamma");
        let alpha_id = self.engine.world().resolve(alpha).map_err(|error| error.to_string())?;
        let beta_id = self.engine.world().resolve(beta).map_err(|error| error.to_string())?;
        let gamma_id = self.engine.world().resolve(gamma).map_err(|error| error.to_string())?;
        self.sync_outliner();
        self.selection
            .replace_many(&[
                selection::SelectionItem::entity(alpha_id).map_err(|error| error.to_string())?,
                selection::SelectionItem::entity(beta_id).map_err(|error| error.to_string())?,
                selection::SelectionItem::entity(gamma_id).map_err(|error| error.to_string())?,
            ])
            .map_err(|error| error.to_string())?;
        self.engine.world_mut().retire_entity(beta).map_err(|error| error.to_string())?;
        self.sync_outliner();
        if self.selection.items().iter().filter_map(|item| item.entity_id()).collect::<Vec<_>>() != vec![alpha_id, gamma_id] {
            return Err("retiring one entity left a stale uuid or dropped a survivor".into());
        }
        if self.selection.primary_entity() != Some(gamma_id) {
            return Err("primary did not stay with the most recent survivor".into());
        }
        let reused = self.engine.world_mut().create_entity("Delta");
        if reused.index != beta.index || reused.generation == beta.generation {
            return Err("slot was not reused".into());
        }
        let reused_id = self.engine.world().resolve(reused).map_err(|error| error.to_string())?;
        self.sync_outliner();
        if self.selection.contains(selection::SelectionItem::Entity(beta_id)) || self.selection.contains(selection::SelectionItem::Entity(reused_id)) {
            return Err("reused slot hijacked selection".into());
        }
        self.engine.world_mut().retire_entity(alpha).map_err(|error| error.to_string())?;
        self.engine.world_mut().retire_entity(gamma).map_err(|error| error.to_string())?;
        self.engine.world_mut().retire_entity(reused).map_err(|error| error.to_string())?;
        self.sync_outliner();
        self.apply_outliner_activation(None, false)?;
        if !self.selection.is_empty() || self.selection.primary_entity().is_some() || !self.inspector_contains("No selection.") {
            return Err("world root did not clear selection".into());
        }
        if self.engine.world().entity_count() != 7 || self.outliner.display_name(near) != Some("Near Triangle") || self.outliner.node(far).and_then(|node| node.parent).is_some() {
            return Err("selection probe did not restore the bootstrap world".into());
        }
        if self.workspace.persistent() != layout || self.workspace.focused() != Some(PERSPECTIVE) {
            return Err("selection probe changed the dock".into());
        }
        if self.panel_hwnd(PERSPECTIVE) != self.viewport_born || self.viewport_view != self.view_born {
            return Err("selection probe changed the viewport".into());
        }
        println!(
            "JRV-0062 probe selection_count=0 selection_revision={} click=PASS ctrl=PASS dock=PASS focus=PASS rename=PASS reparent=PASS retire=PASS reuse=PASS clear=PASS",
            self.selection.revision()
        );
        self.probe_inspector()
    }

    fn inspector_contains(&self, text: &str) -> bool {
        self.inspector_controls.iter().any(|control| window_text(control.hwnd).contains(text))
    }

    fn probe_authored_components(&mut self, outline: &[jarvig_core::EntityOutlineInfo]) -> Result<(), String> {
        let point = outline.iter().find(|row| row.name == "Blue Point Light").ok_or("point light missing")?.uuid;
        let spot = outline.iter().find(|row| row.name == "Warm Spot Light").ok_or("spot light missing")?.uuid;
        let probe = outline.iter().find(|row| row.name == "Reflection Probe").ok_or("probe missing")?.uuid;
        let settings = outline.iter().find(|row| row.name == "World Settings").ok_or("world settings missing")?.uuid;
        self.apply_outliner_activation(Some(point), false)?;
        let intensity = self.inspector_model.field(jarvig_core::TYPE_POINT_LIGHT, jarvig_core::FIELD_INTENSITY).ok_or("point intensity missing")?;
        if intensity.display != "14" || intensity.units != "cd" || !intensity.editable {
            return Err(format!("point intensity was {}", intensity.display));
        }
        self.apply_outliner_activation(Some(spot), false)?;
        let spot_intensity = self.inspector_model.field(jarvig_core::TYPE_SPOT_LIGHT, jarvig_core::FIELD_INTENSITY).ok_or("spot intensity missing")?;
        if spot_intensity.display != "22" {
            return Err(format!("spot intensity was {}", spot_intensity.display));
        }
        self.apply_outliner_activation(Some(probe), false)?;
        let radius = self.inspector_model.field(jarvig_core::TYPE_REFLECTION_PROBE, jarvig_core::FIELD_RADIUS).ok_or("probe radius missing")?;
        let capture = self.inspector_model.field(jarvig_core::TYPE_REFLECTION_PROBE, jarvig_core::FIELD_CAPTURE_STATE).ok_or("probe capture missing")?;
        if radius.display != "8" || capture.editable || capture.display != "Static. Edits do not recapture." {
            return Err("probe inspector did not show the live radius and the static capture".into());
        }
        self.apply_outliner_activation(Some(settings), false)?;
        if self.inspector_model.field(jarvig_core::TYPE_SPATIAL_FRAME, jarvig_core::FIELD_LOCAL_TRANSLATION).is_some() {
            return Err("world settings grew a transform".into());
        }
        let sky = self.inspector_model.field(jarvig_core::TYPE_ENVIRONMENT, jarvig_core::FIELD_INTENSITY).ok_or("environment intensity missing")?;
        if !sky.editable || sky.display != jarvig_core::format_f64(jarvig_core::BOOTSTRAP_ENVIRONMENT_INTENSITY as f64) {
            return Err(format!("environment intensity was {}", sky.display));
        }
        if self.engine.world().object_count() != 2 || self.engine.world().light_count() != 3 {
            return Err("selecting an authorable row changed the renderable world".into());
        }
        Ok(())
    }

    fn probe_inspector(&mut self) -> Result<(), String> {
        let outline = self.engine.world().entity_outline();
        let near = outline[0].uuid;
        let near_handle = self.engine.world().find_entity(near).map_err(|error| error.to_string())?;
        let compiles = self.engine.material_compile_count();
        let meshes = self.renderer.as_ref().map(|renderer| renderer.mesh_upload_count()).unwrap_or(0);
        let textures = self.renderer.as_ref().map(|renderer| renderer.texture_upload_count()).unwrap_or(0);
        self.apply_outliner_activation(Some(near), false)?;
        let selection_revision = self.selection.revision();
        if self.inspector_model.section_count() != 7 || self.inspector_model.field_count() != 16 {
            return Err(format!("inspector sections {} fields {}", self.inspector_model.section_count(), self.inspector_model.field_count()));
        }
        let stack = self.inspector_model.field(jarvig_core::TYPE_COMPONENT_STACK, jarvig_core::FIELD_COMPONENT_STACK).ok_or("component stack missing")?;
        if stack.editable || stack.display != "Transform, Mesh Renderer" {
            return Err(format!("component stack was {}", stack.display));
        }
        let surface = self.inspector_model.field(jarvig_core::TYPE_MESH_RENDERER, jarvig_core::FIELD_SURFACE).ok_or("mesh surface missing")?;
        if surface.editable || surface.display != "Triangle" {
            return Err("mesh surface was editable or wrong".into());
        }
        self.probe_authored_components(&outline)?;
        self.apply_outliner_activation(Some(near), false)?;
        if self.selection.revision() == selection_revision {
            return Err("component probe did not select another entity".into());
        }
        let selection_revision = self.selection.revision();
        let uuid = self.inspector_model.field(jarvig_core::TYPE_ENTITY, jarvig_core::FIELD_UUID).ok_or("uuid field missing")?;
        if uuid.editable || uuid.display != near.to_string() {
            return Err("uuid field was editable or wrong".into());
        }
        let parent = self.inspector_model.field(jarvig_core::TYPE_ENTITY, jarvig_core::FIELD_PARENT).ok_or("parent field missing")?;
        if parent.display != "World" || !parent.editable || !parent.choices.iter().any(|choice| choice == "World") {
            return Err("parent field was not an editable World choice".into());
        }
        let world_before = self.engine.world().revision();
        let renamed = inspector::submit_property(
            &mut self.engine,
            near,
            jarvig_core::TYPE_ENTITY,
            jarvig_core::FIELD_NAME,
            PropertyValue::String("Player Start".into()),
        ).map_err(|error| error.to_string())?;
        if renamed != jarvig_core::AuthoringResult::Applied {
            return Err("name command did not apply".into());
        }
        self.sync_outliner();
        if self.engine.world().find_entity(near).ok() != Some(near_handle) || self.outliner.display_name(near) != Some("Player Start") {
            return Err("name command changed identity or skipped the outliner".into());
        }
        if self.selection.primary_entity() != Some(near) || self.selection.revision() != selection_revision {
            return Err("name command changed selection".into());
        }
        if !self.inspector_contains("Player Start") {
            return Err("inspector did not show the commanded name".into());
        }
        let unchanged = inspector::submit_property(
            &mut self.engine,
            near,
            jarvig_core::TYPE_ENTITY,
            jarvig_core::FIELD_NAME,
            PropertyValue::String("Player Start".into()),
        ).map_err(|error| error.to_string())?;
        if unchanged != jarvig_core::AuthoringResult::Unchanged || self.engine.world().revision() != world_before + 1 {
            return Err("repeating the name command bumped the world".into());
        }
        inspector::submit_property(&mut self.engine, near, jarvig_core::TYPE_ENTITY, jarvig_core::FIELD_NAME, PropertyValue::String("Near Triangle".into()))
            .map_err(|error| error.to_string())?;
        self.sync_outliner();
        let local = match &self.engine.world().inspect_entity(near).map_err(|error| error.to_string())?.sections[1].fields[0].value {
            PropertyValue::Vec3(value) => *value,
            _ => return Err("local translation missing".into()),
        };
        let moved = Vec3::new(local.x + 0.5, local.y, local.z);
        let moved_revision = self.engine.world().revision();
        inspector::submit_property(
            &mut self.engine,
            near,
            jarvig_core::TYPE_SPATIAL_FRAME,
            jarvig_core::FIELD_LOCAL_TRANSLATION,
            PropertyValue::Vec3(moved),
        ).map_err(|error| error.to_string())?;
        if self.engine.world().revision() != moved_revision + 1 {
            return Err("translation command did not bump the world".into());
        }
        let same = inspector::submit_property(
            &mut self.engine,
            near,
            jarvig_core::TYPE_SPATIAL_FRAME,
            jarvig_core::FIELD_LOCAL_TRANSLATION,
            PropertyValue::Vec3(moved),
        ).map_err(|error| error.to_string())?;
        if same != jarvig_core::AuthoringResult::Unchanged {
            return Err("repeating the translation command was not a no-op".into());
        }
        let snapshot = self.engine.world().extract(RenderFrameId(8)).map_err(|error| error.to_string())?;
        let instance = snapshot.instances().iter().find(|instance| instance.entity == near).ok_or("near instance missing")?;
        if (instance.pose.translation.x - (jarvig_core::BOOTSTRAP_ROOT_M + 0.5)).abs() > 1.0e-6 {
            return Err(format!("resolved pose did not move: {}", instance.pose.translation.x));
        }
        let camera = snapshot.camera(self.engine.front_camera().frame).ok_or("camera missing")?;
        let relative = jarvig_core::camera_relative_f32(instance.pose.translation, camera.pose.translation);
        if relative.iter().any(|component| !component.is_finite() || component.abs() > 20.0) {
            return Err(format!("gpu relative position left meter scale: {relative:?}"));
        }
        if self.engine.material_compile_count() != compiles
            || self.renderer.as_ref().map(|renderer| renderer.mesh_upload_count()) != Some(meshes)
            || self.renderer.as_ref().map(|renderer| renderer.texture_upload_count()) != Some(textures)
        {
            return Err("property edit uploaded a mesh, a texture, or a material".into());
        }
        let failures_before = self.engine.authoring_failure_count();
        let revision_before_bad = self.engine.world().revision();
        if inspector::submit_property(
            &mut self.engine,
            near,
            jarvig_core::TYPE_SPATIAL_FRAME,
            jarvig_core::FIELD_LOCAL_TRANSLATION,
            PropertyValue::Vec3(Vec3::new(f64::NAN, 0.0, 0.0)),
        ).is_ok() {
            return Err("NaN translation was accepted".into());
        }
        if inspector::submit_property(
            &mut self.engine,
            near,
            jarvig_core::TYPE_ENTITY,
            jarvig_core::FIELD_UUID,
            PropertyValue::Entity(near),
        ).is_ok() {
            return Err("uuid edit was accepted".into());
        }
        if self.engine.world().revision() != revision_before_bad || self.engine.authoring_failure_count() <= failures_before {
            return Err("rejected edits mutated the world or were not counted".into());
        }
        if self.selection.primary_entity() != Some(near) || self.selection.revision() != selection_revision {
            return Err("failed edit changed selection".into());
        }
        self.workspace.apply(WorkspaceCommand::ClosePanel(INSPECTOR)).map_err(|error| error.to_string())?;
        self.workspace.apply(WorkspaceCommand::ShowPanel(INSPECTOR)).map_err(|error| error.to_string())?;
        self.workspace.apply(WorkspaceCommand::ResetLayout).map_err(|error| error.to_string())?;
        self.realize();
        self.selection_view_ready = false;
        self.sync_selection_view();
        if self.selection.primary_entity() != Some(near) || !self.inspector_contains("Near Triangle") {
            return Err("showing the inspector did not rebuild from the world".into());
        }
        self.workspace.apply(WorkspaceCommand::Focus(PERSPECTIVE)).map_err(|error| error.to_string())?;
        self.authoring_restore = Some((near, local));
        self.authoring_pending_shot = true;
        println!(
            "JRV-0063 probe inspector_sections={} inspector_fields={} name=PASS translation=PASS noop=PASS invalid=PASS readonly=PASS selection=PASS",
            self.inspector_model.section_count(),
            self.inspector_model.field_count()
        );
        Ok(())
    }

    fn nudge_after_resize(&mut self) -> Result<(), String> {
        let near = self.engine.world().entity_outline().first().map(|row| row.uuid).ok_or("near entity missing")?;
        let inspection = self.engine.world().inspect_entity(near).map_err(|error| error.to_string())?;
        let value = inspection
            .sections
            .get(1)
            .and_then(|section| section.fields.first())
            .map(|field| field.value.clone())
            .ok_or("local translation missing")?;
        let PropertyValue::Vec3(local) = value else {
            return Err("local translation missing".into());
        };
        self.authoring_restore = Some((near, local));
        inspector::submit_property(
            &mut self.engine,
            near,
            jarvig_core::TYPE_SPATIAL_FRAME,
            jarvig_core::FIELD_LOCAL_TRANSLATION,
            PropertyValue::Vec3(Vec3::new(local.x + 0.25, local.y, local.z)),
        )
        .map_err(|error| error.to_string())?;
        self.sync_outliner();
        Ok(())
    }

    fn confirm_resize_authoring(&mut self) -> Result<(), String> {
        let (entity, original) = self.authoring_restore.take().ok_or("resize authoring was not armed")?;
        let inspection = self.engine.world().inspect_entity(entity).map_err(|error| error.to_string())?;
        let value = inspection
            .sections
            .get(1)
            .and_then(|section| section.fields.first())
            .map(|field| field.value.clone())
            .ok_or("moved translation missing")?;
        let PropertyValue::Vec3(current) = value else {
            return Err("moved translation missing".into());
        };
        if (current.x - (original.x + 0.25)).abs() > 1.0e-6 {
            return Err("translation after resize did not change the authoritative frame".into());
        }
        if self.engine.material_compile_count() != 1 || self.renderer.as_ref().map(|renderer| renderer.mesh_upload_count()) != Some(2) {
            return Err("resize authoring rebuilt a mesh or a material".into());
        }
        if self.panel_hwnd(PERSPECTIVE) != self.viewport_born || self.viewport_view != self.view_born {
            return Err("resize changed the viewport identity".into());
        }
        inspector::submit_property(
            &mut self.engine,
            entity,
            jarvig_core::TYPE_SPATIAL_FRAME,
            jarvig_core::FIELD_LOCAL_TRANSLATION,
            PropertyValue::Vec3(original),
        )
        .map_err(|error| error.to_string())?;
        self.sync_outliner();
        Ok(())
    }

    fn restore_authoring_pose(&mut self) -> Result<(), String> {
        let Some((entity, local)) = self.authoring_restore.take() else {
            return Err("authoring restore was missing".into());
        };
        inspector::submit_property(
            &mut self.engine,
            entity,
            jarvig_core::TYPE_SPATIAL_FRAME,
            jarvig_core::FIELD_LOCAL_TRANSLATION,
            PropertyValue::Vec3(local),
        ).map_err(|error| error.to_string())?;
        self.sync_outliner();
        let restored = self.engine.world().inspect_entity(entity).map_err(|error| error.to_string())?;
        let PropertyValue::Vec3(value) = restored.sections[1].fields[0].value else {
            return Err("restored translation missing".into());
        };
        if value != local {
            return Err("authoring probe did not restore the local translation".into());
        }
        Ok(())
    }

    fn redraw(&mut self) -> Result<(), String> {
        self.poll_jobs();
        if self.layout_needed {
            self.layout_needed = false;
            self.realize();
        }
        self.sync_outliner();
        self.sync_viewport_drawable()?;
        if self.renderer.is_none() || unsafe { IsIconic(self.frame) } != 0 || !self.perspective_visible() {
            self.end_capture(true, true);
            return Ok(());
        }
        let (width, height) = client_size(self.panel_hwnd(PERSPECTIVE));
        if width == 0 || height == 0 {
            self.end_capture(true, true);
            return Ok(());
        }
        let now = Instant::now();
        let delta = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        if self.camera_probe_pending {
            self.camera_probe_pending = false;
            self.probe_editor_camera()?;
            self.camera_probed = true;
        }
        self.tick_editor_camera(delta)?;
        self.maybe_autosave();
        self.publish_gizmo();
        self.sync_meshlet_debug();
        if self.einstein_debug || self.rfc0002 {
            self.ensure_einstein_uvs();
        }
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_einstein_debug(self.einstein_debug, self.einstein_seed);
            renderer.set_micro_surface(self.micro_surface);
            renderer.set_microgeometry(self.micro_enabled, self.micro_seed, self.micro_color);
        }
        self.publish_land_view();
        self.evict_retired_meshes();
        let target = self.target.ok_or("viewport target missing")?;
        let mut outcome = FrameOutcome::TimedOut;
        let rendered = if self.session_active() {
            self.render_runtime(target, delta, &mut outcome)?
        } else {
            if let Ok(snapshot) = self.engine.world().extract(jarvig_core::RenderFrameId(1)) {
                self.remember_scene_meshlets(&snapshot);
            }
            let tick = {
                let renderer = self.renderer.as_mut().ok_or("renderer missing")?;
                let engine = &mut self.engine;
                engine
                    .run_frame(delta, |snapshot, meshes, materials, textures| {
                        outcome = renderer
                            .render_target(target, snapshot, meshes, materials, textures)
                            .map_err(describe)?;
                        Ok(())
                    })
                    .map_err(describe_frame)?
            };
            tick.render_executed
        };
        if !rendered {
            return Err("editor profile skipped the renderer".into());
        }
        if matches!(outcome, FrameOutcome::Presented { .. }) {
            self.frames = self.frames.saturating_add(1);
            self.on_presented()?;
            self.settle_load();
            self.step_lod_capture();
            self.step_bind_pose_shots();
        }
        self.submit_micro_request();
        if !self.self_test && self.limit.is_some_and(|limit| self.frames >= limit) {
            unsafe {
                KillTimer(self.frame, TIMER_FRAME);
                PostQuitMessage(0);
            }
        }
        Ok(())
    }

    fn sync_viewport_drawable(&mut self) -> Result<(), String> {
        if self.renderer.is_none() {
            return Ok(());
        }
        let suspend = unsafe { IsIconic(self.frame) != 0 } || !self.perspective_visible();
        let (width, height) = if suspend { (0, 0) } else { client_size(self.panel_hwnd(PERSPECTIVE)) };
        let configured = self.renderer.as_ref().unwrap().configured_size();
        if configured == (width, height) {
            return Ok(());
        }
        if self.gizmo_drag.is_some() {
            self.cancel_gizmo();
        }
        self.viewport_resize_requests = self.viewport_resize_requests.saturating_add(1);
        if width == 0 || height == 0 {
            self.viewport_zero_skips = self.viewport_zero_skips.saturating_add(1);
        }
        self.renderer
            .as_mut()
            .unwrap()
            .resize(width, height)
            .map_err(|error| format!("viewport resize {width}x{height} from {}x{}: {error}", configured.0, configured.1))
    }

    fn assert_drawable(&self) -> Result<(), String> {
        if unsafe { IsIconic(self.frame) != 0 } || !self.perspective_visible() {
            return Ok(());
        }
        let (client_w, client_h) = client_size(self.panel_hwnd(PERSPECTIVE));
        let renderer = self.renderer.as_ref().ok_or("renderer missing")?;
        let (surface_w, surface_h) = renderer.configured_size();
        let depth = renderer.depth_size(self.viewport_view.ok_or("view missing")?).map_err(|error| error.to_string())?;
        if client_w == 0 || client_h == 0 {
            if surface_w != 0 || surface_h != 0 {
                return Err(format!("zero client still configured {surface_w}x{surface_h}"));
            }
            return Ok(());
        }
        if (surface_w, surface_h) != (client_w, client_h) {
            return Err(format!("surface {surface_w}x{surface_h} != client {client_w}x{client_h}"));
        }
        if depth != Some((client_w, client_h)) {
            return Err(format!("depth {depth:?} != client {client_w}x{client_h}"));
        }
        if renderer.hdr_scene_size() != Some((client_w, client_h)) {
            return Err(format!("hdr {:?} != client {client_w}x{client_h}", renderer.hdr_scene_size()));
        }
        let aspect = renderer.configured_aspect().ok_or("aspect missing")?;
        let expected = client_w as f32 / client_h as f32;
        if (aspect - expected).abs() > 0.001 {
            return Err(format!("projection aspect {aspect} != {expected}"));
        }
        Ok(())
    }

    fn on_presented(&mut self) -> Result<(), String> {
        if self.born_viewport_px.0 == 0 {
            self.born_viewport_px = self.viewport_px;
        }
        if self.self_test {
            self.assert_drawable()?;
        }
        if !self.self_test || self.phase == Phase::Done || self.posted {
            return Ok(());
        }
        match self.phase {
            Phase::Warmup if self.frames >= 2 => {
                if !self.outliner_probed {
                    self.outliner_probed = true;
                    self.probe_outliner()?;
                    if self.authoring_pending_shot {
                        return Ok(());
                    }
                } else if self.authoring_pending_shot {
                    let _ = capture_window(self.frame, SHOT_AUTHORING);
                    self.restore_authoring_pose()?;
                    self.authoring_pending_shot = false;
                    self.apply_outliner_activation(None, false)?;
                    if !self.inspector_contains("No selection.") {
                        return Err("clearing selection did not restore the empty inspector".into());
                    }
                }
                self.post_test(1);
            }
            Phase::Resized => {
                if self.viewport_px.0 == 0 || self.viewport_px.0 >= self.born_viewport_px.0 {
                    return Err(format!(
                        "resize did not shrink the viewport {} -> {}",
                        self.born_viewport_px.0, self.viewport_px.0
                    ));
                }
                self.post_test(2);
            }
            Phase::Split => self.post_test(3),
            Phase::Tabbed => self.post_test(4),
            Phase::Settling => {
                let start = self.reset_at.unwrap_or(self.frames);
                if self.frames == start.saturating_add(1) && self.authoring_restore.is_none() {
                    self.nudge_after_resize()?;
                }
                if self.frames == start.saturating_add(2) && self.authoring_restore.is_some() {
                    self.confirm_resize_authoring()?;
                    self.camera_probe_pending = true;
                }
                if self.frames >= start.saturating_add(3) && self.camera_probed && self.hdr_probe < 4 {
                    self.advance_hdr_probe()?;
                    return Ok(());
                }
                if self.frames >= start.saturating_add(3) && self.camera_probed && self.env_probe < 11 {
                    self.advance_env_probe()?;
                    return Ok(());
                }
                if self.frames >= start.saturating_add(3) && self.camera_probed && self.spec_probe < 10 {
                    self.advance_spec_probe()?;
                    return Ok(());
                }
                if self.frames >= start.saturating_add(3) && self.camera_probed && self.local_probe < 7 {
                    self.advance_local_probe()?;
                    return Ok(());
                }
                if self.frames >= start.saturating_add(3) && self.camera_probed {
                    unsafe {
                        InvalidateRect(self.toolbar, std::ptr::null(), 1);
                        InvalidateRect(self.panel_hwnd(OUTLINER), std::ptr::null(), 1);
                        InvalidateRect(self.panel_hwnd(INSPECTOR), std::ptr::null(), 1);
                    }
                    let _ = capture_window(self.frame, SHOT_CAMERA);
                    self.restore_presented_gizmo()?;
                    self.reset_editor_camera()?;
                    self.finish_self_test()?;
                    self.phase = Phase::Done;
                    unsafe {
                        KillTimer(self.frame, TIMER_FRAME);
                        PostQuitMessage(0);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn post_test(&mut self, step: usize) {
        self.posted = true;
        unsafe { PostMessageW(self.frame, WM_DOCK_TEST, step, 0); }
    }

    fn on_dock_test(&mut self, step: usize) -> Result<(), String> {
        self.posted = false;
        match step {
            1 => {
                unsafe {
                    SetWindowPos(self.frame, std::ptr::null_mut(), 0, 0, 1100, 700, SWP_NOMOVE | SWP_NOZORDER);
                    SetWindowPos(self.frame, std::ptr::null_mut(), 0, 0, 900, 700, SWP_NOMOVE | SWP_NOZORDER);
                    SetWindowPos(self.frame, std::ptr::null_mut(), 0, 0, 1200, 720, SWP_NOMOVE | SWP_NOZORDER);
                }
                self.phase = Phase::Resized;
            }
            2 => {
                let before = self.panel_dip_width(OUTLINER);
                let split = self.workspace.split_parent(OUTLINER).ok_or("outliner split missing")?;
                self.workspace
                    .apply(WorkspaceCommand::SetSplitRatio { node: split, ratio: 0.30 })
                    .map_err(|error| error.to_string())?;
                self.realize();
                let after = self.panel_dip_width(OUTLINER);
                if after <= before + 1.0 {
                    return Err(format!("splitter did not grow the outliner {before} -> {after}"));
                }
                self.phase = Phase::Split;
            }
            3 => {
                let splits_before = self.workspace.split_count();
                let content = self.workspace.stack_of(CONTENT).ok_or("content stack missing")?;
                self.workspace
                    .apply(WorkspaceCommand::DockPanel { panel: OUTPUT, target: content, zone: DropZone::Center })
                    .map_err(|error| error.to_string())?;
                self.workspace.apply(WorkspaceCommand::Activate(CONTENT)).map_err(|error| error.to_string())?;
                self.workspace.apply(WorkspaceCommand::Activate(OUTPUT)).map_err(|error| error.to_string())?;
                self.realize();
                let output_visible = self.last_layout.visible().any(|panel| panel.panel == OUTPUT);
                let content_visible = self.last_layout.visible().any(|panel| panel.panel == CONTENT);
                let output_stack = self.last_layout.panels.iter().find(|panel| panel.panel == OUTPUT).map(|panel| panel.stack);
                let content_stack = self.last_layout.panels.iter().find(|panel| panel.panel == CONTENT).map(|panel| panel.stack);
                if !output_visible || content_visible || output_stack != content_stack {
                    return Err("tab activation did not hide the inactive content panel".into());
                }
                if self.workspace.split_count() >= splits_before {
                    return Err("docking the last tab did not collapse the empty stack".into());
                }
                if self.workspace.focused() != Some(OUTPUT) {
                    return Err("activating the output tab did not focus it".into());
                }
                if self.panel_hwnd(PERSPECTIVE) != self.viewport_born || self.viewport_view != self.view_born {
                    return Err("viewport identity changed while docking a tab".into());
                }
                if self.renderer.as_ref().map(|renderer| renderer.view_count()) != Some(1) {
                    return Err("docking changed the render view count".into());
                }
                let _ = capture_window(self.frame, SHOT_TABS);
                self.phase = Phase::Tabbed;
            }
            4 => {
                self.workspace.apply(WorkspaceCommand::ClosePanel(OUTPUT)).map_err(|error| error.to_string())?;
                if self.workspace.is_open(OUTPUT) {
                    return Err("close left the output panel open".into());
                }
                self.workspace.apply(WorkspaceCommand::ShowPanel(OUTPUT)).map_err(|error| error.to_string())?;
                if !self.workspace.is_open(OUTPUT) {
                    return Err("show did not reopen the output panel".into());
                }
                let expected = DockWorkspace::default_layout().persistent();
                self.workspace.apply(WorkspaceCommand::ResetLayout).map_err(|error| error.to_string())?;
                if self.workspace.persistent() != expected {
                    return Err("reset layout did not restore the default workspace".into());
                }
                if self.workspace.focused() != Some(PERSPECTIVE) {
                    return Err("reset did not return focus to the perspective viewport".into());
                }
                let world_before_layout = self.engine.world().revision();
                if let Some(controller) = self.editor_camera.as_mut() {
                    controller.fly(0, 0, 1, 1.0, 0.2);
                }
                let raised = self.editor_camera.as_ref().map(|controller| controller.position.y).ok_or("camera missing before layout reset")?;
                self.workspace.apply(WorkspaceCommand::ResetLayout).map_err(|error| error.to_string())?;
                let raised_after = self.editor_camera.as_ref().map(|controller| controller.position.y).ok_or("camera missing after layout reset")?;
                if (raised_after - raised).abs() > 1.0e-9 || self.engine.world().revision() != world_before_layout {
                    return Err("reset layout changed the editor camera or the world".into());
                }
                if self.panel_hwnd(PERSPECTIVE) != self.viewport_born || self.viewport_view != self.view_born {
                    return Err("reset recreated the viewport".into());
                }
                self.realize();
                unsafe { ShowWindow(self.frame, SW_MINIMIZE); }
                if unsafe { IsIconic(self.frame) } != 0 {
                    self.sync_viewport_drawable()?;
                    if self.renderer.as_ref().map(|renderer| renderer.configured_size()) != Some((0, 0)) {
                        return Err("minimized editor kept a drawable surface".into());
                    }
                }
                unsafe { ShowWindow(self.frame, SW_RESTORE); }
                self.realize();
                self.sync_view_menu();
                self.reset_at = Some(self.frames);
                self.phase = Phase::Settling;
            }
            _ => return Err(format!("unknown dock test step {step}")),
        }
        Ok(())
    }

    fn finish_self_test(&mut self) -> Result<(), String> {
        let renderer = self.renderer.as_ref().ok_or("renderer missing")?;
        if self.viewport_view != self.view_born {
            return Err("viewport view changed".into());
        }
        if self.panel_hwnd(PERSPECTIVE) != self.viewport_born {
            return Err("viewport hwnd changed".into());
        }
        if renderer.view_count() != 1
            || self.engine.material_compile_count() != 1
            || self.engine.world_light_count() != 3
            || self.engine.mesh_count() != 2
            || renderer.presents_last_frame() != 1
            || self.workspace.panel_count() != 5
            || self.workspace.stack_count() != 5
            || self.workspace.split_count() != 4
            || self.workspace.schema_version() != SCHEMA_VERSION
        {
            return Err(format!(
                "views={} compiles={} lights={} meshes={} presents={} panels={} stacks={} splits={}",
                renderer.view_count(),
                self.engine.material_compile_count(),
                self.engine.world_light_count(),
                self.engine.mesh_count(),
                renderer.presents_last_frame(),
                self.workspace.panel_count(),
                self.workspace.stack_count(),
                self.workspace.split_count()
            ));
        }
        println!(
            "JARVIGEditor dock_panels={} dock_stacks={} dock_splits={} active={} views=1 objects={} lights={} compiles={} presents_last={}",
            self.workspace.panel_count(),
            self.workspace.stack_count(),
            self.workspace.split_count(),
            self.workspace.focused().unwrap_or(PERSPECTIVE),
            self.engine.world().object_count(),
            self.engine.world_light_count(),
            self.engine.material_compile_count(),
            renderer.presents_last_frame()
        );
        println!(
            "JRV-0059 behavior splitter=PASS tabs=PASS show_hide=PASS dock=PASS collapse=PASS reset=PASS focus=PASS viewport=PASS"
        );
        let primary = self.selection.primary_entity().map(|id| id.to_string()).unwrap_or_else(|| "none".into());
        println!(
            "JRV-0061 outliner entity_count={} outliner_nodes={} outliner_roots={} outliner_revision={} objects={}",
            self.engine.world().entity_count(),
            self.outliner.entity_count(),
            self.outliner.root_entity_count(),
            self.outliner.revision(),
            self.engine.world().object_count()
        );
        println!(
            "JRV-0062 selection selection_count={} selection_revision={} selection_primary={}",
            self.selection.count(),
            self.selection.revision(),
            primary
        );
        let (surface_w, surface_h) = renderer.configured_size();
        let (client_w, client_h) = client_size(self.panel_hwnd(PERSPECTIVE));
        let depth = renderer.depth_size(self.viewport_view.ok_or("view missing")?).map_err(|error| error.to_string())?;
        println!(
            "JRV-0063 inspector inspector_sections={} inspector_fields={} inspection_world_revision={} authoring_commands={} authoring_command_failures={}",
            self.inspector_model.section_count(),
            self.inspector_model.field_count(),
            self.inspector_model.world_revision,
            self.engine.authoring_command_count(),
            self.engine.authoring_failure_count()
        );
        println!(
            "JRV-0070 viewport viewport_client_px={client_w}x{client_h} surface_configured_px={surface_w}x{surface_h} depth_px={}x{} projection_aspect={} viewport_resize_requests={} viewport_resize_applied={} viewport_resize_failures={} viewport_zero_size_skips={}",
            depth.map(|size| size.0).unwrap_or(0),
            depth.map(|size| size.1).unwrap_or(0),
            renderer.configured_aspect().unwrap_or(0.0),
            self.viewport_resize_requests,
            renderer.resize_applied(),
            renderer.resize_failures(),
            self.viewport_zero_skips
        );
        if !self.camera_report.is_empty() {
            println!("{}", self.camera_report);
        }
        let hover = self
            .gizmo_hover
            .filter(|handle| handle.is_rotation() == (self.object_tool == chrome::ToolbarCommand::Rotate))
            .map(|handle| handle.label())
            .unwrap_or("none");
        let active = self.gizmo_drag.as_ref().map(|drag| drag.handle.label()).unwrap_or("none");
        let visible = if self.gizmo_vertices().is_some() { 1 } else { 0 };
        let editing = if self.gizmo_drag.is_some() { 1 } else { 0 };
        println!(
            "JRV-0065 gizmo editor_tool_mode={} pick_requests={} pick_hits={} pick_misses={} gizmo_visible={visible} gizmo_hover_handle={hover} gizmo_active_handle={active} authoring_edit_active={editing} gizmo_updates={} gizmo_commits={} gizmo_cancels={} instance_transform_upload_count={}",
            self.object_tool.label(),
            self.pick_requests,
            self.pick_hits,
            self.pick_misses,
            self.gizmo_updates,
            self.gizmo_commits,
            self.gizmo_cancels,
            renderer.instance_transform_upload_count()
        );
        let exposure = self.viewport_view.and_then(|view| renderer.exposure_ev(view).ok()).unwrap_or(0.0);
        let (hdr_w, hdr_h) = renderer.hdr_scene_size().unwrap_or((0, 0));
        println!(
            "JRV-0071 hdr hdr_scene_format={} hdr_scene_px={hdr_w}x{hdr_h} hdr_scene_target_count={} exposure_ev={exposure:.1} exposure_multiplier={:.4} tone_map_pipeline_count={} tone_map_parameter_upload_count={} output_pass_count={}",
            renderer.hdr_scene_format(),
            renderer.hdr_scene_target_count(),
            jarvig_renderer::exposure_multiplier(exposure),
            renderer.tone_map_pipeline_count(),
            renderer.tone_map_parameter_upload_count(),
            renderer.output_pass_count()
        );
        if !self.env_report.is_empty() {
            println!("{}", self.env_report);
        }
        if !self.spec_report.is_empty() {
            println!("{}", self.spec_report);
        }
        if !self.local_report.is_empty() {
            println!("{}", self.local_report);
        }
        let shadows = renderer.shadow_diagnostics();
        if shadows.map_count != 3
            || shadows.directional_resolution != 1024
            || shadows.spot_resolution != 1024
            || shadows.point_resolution != 256
            || shadows.update_count == 0
            || renderer.mesh_upload_count() != 2
            || renderer.shadow_cull() != Some(jarvig_rhi::CullMode::None)
        {
            return Err(format!(
                "shadows maps={} dir={} spot={} point={} updates={} meshes={} cull={:?}",
                shadows.map_count,
                shadows.directional_resolution,
                shadows.spot_resolution,
                shadows.point_resolution,
                shadows.update_count,
                renderer.mesh_upload_count(),
                renderer.shadow_cull()
            ));
        }
        if renderer.indirect_diffuse_texture_count() != 1 || renderer.reflection_probe_capture_count() != 1 {
            return Err(format!(
                "indirect textures={} captures={}",
                renderer.indirect_diffuse_texture_count(),
                renderer.reflection_probe_capture_count()
            ));
        }
        println!(
            "JRV-0075 indirect indirect_diffuse_texture_count={} indirect_diffuse_samples=64 capture_count={}",
            renderer.indirect_diffuse_texture_count(),
            renderer.reflection_probe_capture_count()
        );
        println!(
            "JRV-0077 shadows shadow_map_count={} shadow_directional_px={} shadow_spot_px={} shadow_point_px={} shadow_update_count={} shadow_cull=none",
            shadows.map_count, shadows.directional_resolution, shadows.spot_resolution, shadows.point_resolution, shadows.update_count
        );
        println!(
            "JRV-0074 probe reflection_probe_count={} reflection_probe_active_count={} reflection_probe_capture_count={} reflection_probe_selected_far={} reflection_probe_fallback_count={} reflection_probe_resolution={} reflection_probe_mip_count={}",
            renderer.reflection_probe_count(),
            renderer.reflection_probe_active_count(),
            renderer.reflection_probe_capture_count(),
            renderer.reflection_probe_selected_far(),
            renderer.reflection_probe_fallback_count(),
            renderer.reflection_probe_resolution(),
            renderer.reflection_probe_mip_count()
        );
        let environment = self.engine.world().environment();
        let near_fill = self.near_normal().map(|normal| self.unit_environment(normal, 0.0, 1.0)).unwrap_or([0.0; 3]);
        println!(
            "JRV-0072 environment environment_light_count={} environment_enabled={} environment_intensity={:.3} environment_upper_rgb={:.3},{:.3},{:.3} environment_lower_rgb={:.3},{:.3},{:.3} environment_packet_upload_count={} environment_diffuse_near={:.4},{:.4},{:.4}",
            self.engine.world().environment_light_count(),
            u32::from(environment.enabled),
            environment.intensity,
            environment.upper_hemisphere_linear_rgb[0],
            environment.upper_hemisphere_linear_rgb[1],
            environment.upper_hemisphere_linear_rgb[2],
            environment.lower_hemisphere_linear_rgb[0],
            environment.lower_hemisphere_linear_rgb[1],
            environment.lower_hemisphere_linear_rgb[2],
            renderer.environment_packet_upload_count(),
            near_fill[0],
            near_fill[1],
            near_fill[2]
        );
        let far_spec = self.far_reflection(0.2).unwrap_or([0.0; 3]);
        println!(
            "JRV-0073 specular environment_specular_enabled={} environment_specular_model=analytical environment_specular_far={:.4},{:.4},{:.4} environment_reflection_updates={}",
            u32::from(environment.enabled),
            far_spec[0],
            far_spec[1],
            far_spec[2],
            self.spec_updates
        );
        match capture_window(self.frame, SHOT_PATH) {
            Ok(()) => println!("JARVIGEditor shot={SHOT_PATH}"),
            Err(error) => println!("JARVIGEditor shot skipped: {error}"),
        }
        Ok(())
    }

    fn realize(&mut self) {
        if self.frame.is_null() || self.dock_host.is_null() {
            return;
        }
        let dpi = unsafe { GetDpiForWindow(self.frame) }.max(96);
        self.dpi = dpi;
        self.ensure_font();
        let mut rect: RECT = unsafe { std::mem::zeroed() };
        unsafe { GetClientRect(self.frame, &mut rect); }
        let width = rect.right;
        let height = rect.bottom;
        let toolbar = dip_to_px(TOOLBAR_DIP, dpi).max(1);
        let status = dip_to_px(STATUS_DIP, dpi).max(1);
        let dock_top = toolbar;
        let dock_height = (height - toolbar - status).max(1);
        place(self.toolbar, 0, 0, width, toolbar);
        place(self.status, 0, height - status, width, status);
        place(self.dock_host, 0, dock_top, width, dock_height);
        let viewport_origin_y = dock_top;
        let bounds = DipRect {
            x: 0.0,
            y: 0.0,
            width: px_to_dip(width, dpi).max(1.0),
            height: px_to_dip(dock_height, dpi).max(1.0),
        };
        self.last_layout = self.workspace.layout(bounds);
        for (id, hwnd) in self.panels {
            let placed = self.last_layout.panels.iter().find(|panel| panel.panel == id && panel.visible);
            if let Some(panel) = placed {
                let px = dip_rect_px(panel.rect, dpi);
                let (x, y) = if id == PERSPECTIVE {
                    (px.left, viewport_origin_y + px.top)
                } else {
                    (px.left, px.top)
                };
                let hold_swapchain = id == PERSPECTIVE && self.renderer.is_some() && self.frames == 0;
                if !hold_swapchain {
                    if id == PERSPECTIVE {
                        unsafe {
                            SetWindowPos(
                                hwnd,
                                std::ptr::null_mut(),
                                x,
                                y,
                                (px.right - px.left).max(1),
                                (px.bottom - px.top).max(1),
                                SWP_NOACTIVATE,
                            );
                        }
                    } else {
                        place(hwnd, x, y, (px.right - px.left).max(1), (px.bottom - px.top).max(1));
                    }
                }
                unsafe { ShowWindow(hwnd, SW_SHOW); }
            } else if !hwnd.is_null() {
                unsafe { ShowWindow(hwnd, SW_HIDE); }
            }
        }
        if self.perspective_visible() {
            let (width, height) = client_size(self.panel_hwnd(PERSPECTIVE));
            self.viewport_px = (width, height);
        }
        unsafe { InvalidateRect(self.dock_host, std::ptr::null(), 1); }
        self.place_load_overlay();
    }

    fn perspective_visible(&self) -> bool {
        self.last_layout.visible().any(|panel| panel.panel == PERSPECTIVE)
    }

    fn panel_hwnd(&self, id: PanelId) -> HWND {
        self.panels.iter().find(|(panel, _)| *panel == id).map(|(_, hwnd)| *hwnd).unwrap_or(std::ptr::null_mut())
    }

    fn panel_dip_width(&self, id: PanelId) -> f32 {
        self.last_layout.visible().find(|panel| panel.panel == id).map(|panel| panel.rect.width).unwrap_or(0.0)
    }

    fn toggle_panel(&mut self, panel: PanelId) {
        let command = if self.workspace.is_open(panel) {
            WorkspaceCommand::ClosePanel(panel)
        } else {
            WorkspaceCommand::ShowPanel(panel)
        };
        if let Err(error) = self.workspace.apply(command) {
            self.append(&error.to_string());
        }
        self.realize();
        self.sync_view_menu();
    }

    fn reset_from_menu(&mut self) {
        if let Err(error) = self.workspace.apply(WorkspaceCommand::ResetLayout) {
            self.append(&error.to_string());
            return;
        }
        self.realize();
        self.sync_view_menu();
        self.append("Layout reset. The world was not changed.");
    }

    fn sync_view_menu(&self) {
        if self.view_menu.is_null() {
            return;
        }
        for (command, panel) in [
            (ID_VIEW_OUTLINER, OUTLINER),
            (ID_VIEW_INSPECTOR, INSPECTOR),
            (ID_VIEW_CONTENT, CONTENT),
            (ID_VIEW_OUTPUT, OUTPUT),
        ] {
            let flags = MF_BYCOMMAND | if self.workspace.is_open(panel) { MF_CHECKED } else { MF_UNCHECKED };
            unsafe { CheckMenuItem(self.view_menu, command as u32, flags); }
        }
        let environment = MF_BYCOMMAND | if self.engine.world().environment().enabled { MF_CHECKED } else { MF_UNCHECKED };
        unsafe { CheckMenuItem(self.view_menu, ID_VIEW_ENVIRONMENT as u32, environment); }
        let pilot = MF_BYCOMMAND | if self.pilot_entity.is_some() { MF_CHECKED } else { MF_UNCHECKED };
        unsafe { CheckMenuItem(self.view_menu, ID_VIEW_PILOT as u32, pilot); }
        let mark_quality = |id: usize, on: bool| {
            let flags = MF_BYCOMMAND | if on { MF_CHECKED } else { MF_UNCHECKED };
            unsafe { CheckMenuItem(self.view_menu, id as u32, flags); }
        };
        mark_quality(ID_QUALITY_BASELINE, self.render_quality == jarvig_core::RenderQuality::Baseline);
        mark_quality(ID_QUALITY_ENHANCED, self.render_quality == jarvig_core::RenderQuality::Enhanced);
        mark_quality(ID_QUALITY_HIGH, self.render_quality == jarvig_core::RenderQuality::High);
        mark_quality(ID_PRESENT_DITHER, self.output_dither);
        mark_quality(ID_PRESENT_TONEMAP, self.presentation == PresentationMode::Tonemap);
        mark_quality(ID_PRESENT_QUANTIZED, self.presentation == PresentationMode::Quantized);
        mark_quality(ID_PRESENT_BEFORE, self.presentation == PresentationMode::BeforeCurve);
        mark_quality(ID_VIEW_CHARACTER, self.character_workspace);
        mark_quality(ID_VIEW_LAND, self.land_mode);
        mark_quality(ID_VIEW_JOINTS, self.character_workspace && self.show_joint_debug);
        mark_quality(ID_VIEW_ALL_JOINTS, self.character_workspace && self.show_all_joints);
        mark_quality(ID_VIEW_JOINT_LIMITS, self.character_workspace && self.show_joint_limits);
        mark_quality(ID_VIEW_MESHLETS, self.meshlet_debug);
        mark_quality(ID_VIEW_MESHLET_SHADE, self.meshlet_shade);
        mark_quality(ID_VIEW_MESHLET_FRUSTUM, self.meshlet_frustum);
        mark_quality(ID_VIEW_MESHLET_OCCLUSION, self.meshlet_occlusion);
        mark_quality(ID_VIEW_MESHLET_FREEZE, self.meshlet_freeze);
        mark_quality(ID_VIEW_MESHLET_HIGHLIGHT, self.meshlet_highlight);
        mark_quality(ID_VIEW_CLUSTER_HIERARCHY, self.cluster_hierarchy);
        mark_quality(ID_VIEW_EINSTEIN, self.einstein_debug);
        mark_quality(ID_VIEW_MICRO, self.micro_enabled);
        mark_quality(ID_VIEW_MICRO_COLOR, self.micro_color);
        mark_quality(ID_VIEW_ERROR_HALF, (self.hierarchy_error_px - 0.5).abs() < 0.01);
        mark_quality(ID_VIEW_ERROR_ONE, (self.hierarchy_error_px - 1.0).abs() < 0.01);
        mark_quality(ID_VIEW_ERROR_TWO, (self.hierarchy_error_px - 2.0).abs() < 0.01);
        mark_quality(ID_VIEW_ERROR_FOUR, (self.hierarchy_error_px - 4.0).abs() < 0.01);
        self.sync_lighting_menu();
    }

    fn sync_lighting_menu(&self) {
        if self.lighting_menu.is_null() {
            return;
        }
        let debug = self.lighting_debug;
        let mark = |id: usize, on: bool| {
            let flags = MF_BYCOMMAND | if on { MF_CHECKED } else { MF_UNCHECKED };
            unsafe { CheckMenuItem(self.lighting_menu, id as u32, flags); }
        };
        mark(ID_DEBUG_FULL, debug == LightingDebug::default());
        mark(ID_DEBUG_DIRECT, debug == LightingDebug::direct_only());
        mark(ID_DEBUG_ENV_DIFFUSE, debug == LightingDebug::environment_diffuse_only());
        mark(ID_DEBUG_ENV_SPECULAR, debug == LightingDebug::environment_specular_only());
        mark(ID_DEBUG_PROBE_ONLY, debug == LightingDebug::probe_specular_only());
        mark(ID_DEBUG_EMISSIVE, debug == LightingDebug::emissive_only());
        mark(ID_DEBUG_DIRECTIONAL, debug.directional);
        mark(ID_DEBUG_POINT, debug.point);
        mark(ID_DEBUG_SPOT, debug.spot);
        mark(ID_DEBUG_GLOBAL_ENV, debug.env_diffuse || debug.env_specular);
        mark(ID_DEBUG_PROBE, debug.probe);
        mark(ID_DEBUG_DIRECTIONAL_ONLY, debug == LightingDebug::directional_only());
        mark(ID_DEBUG_POINT_ONLY, debug == LightingDebug::point_only());
        mark(ID_DEBUG_SPOT_ONLY, debug == LightingDebug::spot_only());
        mark(ID_DEBUG_NO_SHADOWS, debug == LightingDebug::shadows_disabled());
        mark(ID_DEBUG_INDIRECT_ONLY, debug == LightingDebug::indirect_diffuse_only());
        mark(ID_DEBUG_DIRECT_UNSHADOWED, debug == LightingDebug::direct_unshadowed());
        mark(ID_DEBUG_CASCADES, self.cascade_debug);
        mark(ID_DEBUG_CONTACT, self.contact_shadows);
        mark(ID_MAT_FULL, self.material_channel == 0);
        mark(ID_MAT_BASE, self.material_channel == 1);
        mark(ID_MAT_NORMAL, self.material_channel == 2);
        mark(ID_MAT_ROUGH, self.material_channel == 3);
        mark(ID_MAT_AO, self.material_channel == 4);
        mark(ID_MAT_METAL, self.material_channel == 5);
    }

    fn view_settings(&self, exposure_ev: f32, lighting: LightingDebug) -> RenderViewSettings {
        RenderViewSettings {
            exposure_ev,
            lighting,
            cascade_debug: self.cascade_debug,
            contact_shadows: self.contact_shadows,
            output_dither: self.output_dither,
            presentation: self.presentation,
            material_channel: self.material_channel,
        }
    }

    fn material_channel_label(channel: u32) -> &'static str {
        match channel {
            1 => "Base Color",
            2 => "Normal",
            3 => "Roughness",
            4 => "AO",
            5 => "Metallic",
            _ => "Full",
        }
    }

    fn set_material_channel(&mut self, channel: u32) {
        let compiles = self.engine.material_compile_count();
        let revision = self.engine.world().revision();
        self.material_channel = channel;
        let _ = self.push_view_settings();
        if self.engine.material_compile_count() != compiles || self.engine.world().revision() != revision {
            self.append("Material channel changed a shader or the level. That is a bug. The view was not supposed to.");
        }
        self.append(&format!(
            "Material view is {}. It does not rewrite the material or the level.",
            Self::material_channel_label(channel)
        ));
    }

    fn push_view_settings(&mut self) -> Result<(), String> {
        let revision = self.engine.world().revision();
        let exposure = self
            .viewport_view
            .and_then(|view| self.renderer.as_ref().and_then(|renderer| renderer.exposure_ev(view).ok()))
            .unwrap_or(0.0);
        let settings = self.view_settings(exposure, self.lighting_debug);
        let view = self.viewport_view.ok_or("viewport view missing")?;
        let renderer = self.renderer.as_mut().ok_or("renderer missing")?;
        renderer
            .update_view(view, RenderViewUpdate { camera: None, layout: None, settings: Some(settings), pose: None })
            .map_err(|error| error.to_string())?;
        if self.engine.world().revision() != revision {
            return Err("presentation view revised the world".into());
        }
        self.sync_view_menu();
        self.refresh_status();
        Ok(())
    }

    fn set_presentation(&mut self, mode: PresentationMode) {
        self.presentation = mode;
        let _ = self.push_view_settings();
        self.append(&format!(
            "Presentation is {}. Dither is {}. Shadows stay on the Lighting Debug menu. Contact Shadows stays there too. Not saved.",
            mode.label(),
            if self.output_dither { "on" } else { "off" }
        ));
    }

    fn apply_lighting_debug(&mut self, debug: LightingDebug) -> Result<(), String> {
        let revision = self.engine.world().revision();
        let compiles = self.engine.material_compile_count();
        let meshes = self.renderer.as_ref().map(|renderer| renderer.mesh_upload_count()).unwrap_or(0);
        let textures = self.renderer.as_ref().map(|renderer| renderer.texture_upload_count()).unwrap_or(0);
        let exposure = self
            .viewport_view
            .and_then(|view| self.renderer.as_ref().and_then(|renderer| renderer.exposure_ev(view).ok()))
            .unwrap_or(0.0);
        self.lighting_debug = debug;
        let settings = self.view_settings(exposure, debug);
        let view = self.viewport_view.ok_or("viewport view missing")?;
        let renderer = self.renderer.as_mut().ok_or("renderer missing")?;
        renderer
            .update_view(
                view,
                RenderViewUpdate {
                    camera: None,
                    layout: None,
                    settings: Some(settings),
                    pose: None,
                },
            )
            .map_err(|error| error.to_string())?;
        if self.engine.world().revision() != revision {
            return Err("lighting debug revised the world".into());
        }
        let renderer = self.renderer.as_ref().ok_or("renderer missing")?;
        if self.engine.material_compile_count() != compiles
            || renderer.mesh_upload_count() != meshes
            || renderer.texture_upload_count() != textures
        {
            return Err("lighting debug rebuilt a material, mesh, or texture".into());
        }
        self.sync_lighting_menu();
        self.refresh_status();
        self.append(&format!("Lighting debug: {}. The world was not revised.", debug.label()));
        Ok(())
    }

    fn paint_dock(&mut self, hwnd: HWND) {
        unsafe {
            let mut paint: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut paint);
            if !hdc.is_null() {
                let font = GetStockObject(DEFAULT_GUI_FONT);
                SelectObject(hdc, font);
                SetBkMode(hdc, TRANSPARENT as i32);
                let mut client: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut client);
                FillRect(hdc, &client, chrome::chrome_brush());
                if !self.ui_font.is_null() {
                    SelectObject(hdc, self.ui_font);
                }
                let accent = CreateSolidBrush(chrome::ACCENT);
                let splitter = CreateSolidBrush(chrome::SPLITTER);
                let idle = CreateSolidBrush(chrome::TAB_IDLE);
                for stack in &self.last_layout.stacks {
                    for tab in &stack.tabs {
                        let px = dip_rect_px(tab.rect, self.dpi);
                        FillRect(hdc, &px, if tab.active { chrome::panel_brush() } else { idle });
                        if tab.active {
                            let mark = RECT { left: px.left, top: px.top, right: px.right, bottom: px.top + 2 };
                            FillRect(hdc, &mark, accent);
                            SetTextColor(hdc, chrome::TEXT);
                        } else {
                            SetTextColor(hdc, chrome::TEXT_DIM);
                        }
                        let label = self.tab_title(tab.panel);
                        let title = wide(&label);
                        TextOutW(hdc, px.left + 8, px.top + 4, title.as_ptr(), (title.len() - 1) as i32);
                    }
                }
                for splitter_rect in &self.last_layout.splitters {
                    let px = dip_rect_px(splitter_rect.rect, self.dpi);
                    FillRect(hdc, &px, splitter);
                }
                DeleteObject(accent);
                DeleteObject(splitter);
                DeleteObject(idle);
            }
            EndPaint(hwnd, &paint);
        }
    }

    fn dock_mouse(&mut self, message: u32, lparam: LPARAM) {
        let point = point_from_lparam(lparam, self.dpi);
        match message {
            WM_LBUTTONDOWN => {
                if let Some(node) = self.last_layout.splitter_at(point) {
                    self.drag = Some(Drag::Splitter { node });
                    unsafe { SetCapture(self.dock_host); }
                    return;
                }
                if let Some(panel) = self.last_layout.tab_at(point) {
                    let _ = self.workspace.apply(WorkspaceCommand::Activate(panel));
                    self.text_focused = false;
                    self.drag = Some(Drag::Tab { panel, start: point, armed: false });
                    unsafe { SetCapture(self.dock_host); }
                    self.realize();
                    self.sync_view_menu();
                }
            }
            WM_MOUSEMOVE => match &self.drag {
                Some(Drag::Splitter { node }) => {
                    let node = *node;
                    if let Some(ratio) = self.last_layout.ratio_at(node, point) {
                        let _ = self.workspace.apply(WorkspaceCommand::SetSplitRatio { node, ratio });
                        self.realize();
                    }
                }
                Some(Drag::Tab { panel, start, armed }) => {
                    let panel = *panel;
                    let start = *start;
                    let armed = *armed || (point.x - start.x).abs() + (point.y - start.y).abs() > 4.0;
                    if let Some(Drag::Tab { armed: slot, .. }) = &mut self.drag {
                        *slot = armed;
                    }
                    if armed {
                        let target = self.last_layout.drop_target(point);
                        if let Some(target) = target {
                            self.place_highlight(Some(target.rect));
                        }
                        let _ = panel;
                    }
                }
                None => {}
            },
            WM_LBUTTONUP => {
                let drag = self.drag.take();
                unsafe { ReleaseCapture(); }
                if let Some(Drag::Tab { panel, armed: true, .. }) = drag {
                    if let Some(target) = self.last_layout.drop_target(point) {
                        let _ = self.workspace.apply(WorkspaceCommand::DockPanel {
                            panel,
                            target: target.stack,
                            zone: target.zone,
                        });
                    }
                }
                self.place_highlight(None);
                self.realize();
                self.sync_view_menu();
            }
            _ => {}
        }
    }

    fn place_highlight(&self, rect: Option<DipRect>) {
        if self.highlight.is_null() {
            return;
        }
        unsafe {
            if let Some(rect) = rect {
                let px = dip_rect_px(rect, self.dpi);
                SetWindowPos(
                    self.highlight,
                    std::ptr::null_mut(),
                    px.left,
                    px.top,
                    (px.right - px.left).max(1),
                    (px.bottom - px.top).max(1),
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            } else {
                ShowWindow(self.highlight, SW_HIDE);
            }
        }
    }

    fn cursor_for_dock(&self) -> bool {
        unsafe {
            let mut point = POINT { x: 0, y: 0 };
            if GetCursorPos(&mut point) == 0 || ScreenToClient(self.dock_host, &mut point) == 0 {
                return false;
            }
            let dip = DipPoint { x: px_to_dip(point.x, self.dpi), y: px_to_dip(point.y, self.dpi) };
            let Some(node) = self.last_layout.splitter_at(dip) else {
                return false;
            };
            let axis = self.last_layout.splitters.iter().find(|splitter| splitter.node == node).map(|splitter| splitter.axis);
            let cursor = match axis {
                Some(Axis::Vertical) => IDC_SIZENS,
                _ => IDC_SIZEWE,
            };
            SetCursor(LoadCursorW(std::ptr::null_mut(), cursor));
            true
        }
    }

    fn note_child_focus(&mut self, id: u32) {
        let panel = PanelId(id);
        if panel != PERSPECTIVE {
            self.end_capture(true, true);
        }
        if self.workspace.apply(WorkspaceCommand::Focus(panel)).is_ok() {
            self.text_focused = panel_takes_text(panel);
        }
    }

    fn refresh_status(&self) {
        let speed = self
            .editor_camera
            .as_ref()
            .map(|controller| format!("{:.1} m/s", controller.speed_m_s))
            .unwrap_or_else(|| "—".into());
        let observed = selection::status_selection(&self.selection, |id| {
            self.outliner.display_name(id).unwrap_or("Missing").to_string()
        });
        let space = match self.transform_space {
            gizmo::TransformSpace::World => "world",
            gizmo::TransformSpace::Local => "local",
        };
        let exposure = self
            .viewport_view
            .and_then(|view| self.renderer.as_ref().and_then(|renderer| renderer.exposure_ev(view).ok()))
            .unwrap_or(0.0);
        let debug = if self.lighting_debug == LightingDebug::default() {
            String::new()
        } else {
            format!("  |  Debug: {}", self.lighting_debug.label())
        };
        let material_view = if self.material_channel == 0 {
            String::new()
        } else {
            format!("  |  Material: {}", Self::material_channel_label(self.material_channel))
        };
        let shadows = self
            .renderer
            .as_ref()
            .map(|renderer| {
                let diag = renderer.shadow_diagnostics();
                format!(
                    "cascades {} filter {} contact {} updated {} draws {} casters {} cpu {:.2} ms dir {} spot {} point {}",
                    diag.directional_cascades,
                    if diag.filter_pcss == 1 { "pcss" } else { "pcf" },
                    diag.contact_shadow_enabled,
                    diag.maps_updated,
                    diag.shadow_draws,
                    diag.shadow_casters,
                    diag.shadow_pass_ms,
                    diag.directional_resolution,
                    diag.spot_resolution,
                    diag.point_resolution
                )
            })
            .unwrap_or_else(|| "not ready".into());
        let progress = self.progress.status_line();
        let progress = if progress.is_empty() { String::new() } else { format!("{progress}  |  ") };
        let view_name = if self.session_active() {
            "Game"
        } else if self.pilot_entity.is_some() {
            "Pilot"
        } else {
            "Perspective"
        };
        let play_state = if self.session_active() {
            let tick = self.game.runtime_world().map(|world| world.simulation_tick()).unwrap_or(0);
            let seconds = tick as f64 / 60.0;
            let name = if self.play == PlayPhase::Paused { "PAUSED" } else { "PLAY" };
            format!("{name} tick {tick} {seconds:.2}s  |  ")
        } else {
            String::new()
        };
        let jobs = if !self.drop_feedback.is_empty() {
            format!("{}  |  ", self.drop_feedback)
        } else if self.job_line.is_empty() {
            String::new()
        } else {
            format!("{}  |  ", self.job_line)
        };
        let micro = self.micro_status();
        let meshlets = self.meshlet_status();
        let scene = self.gpu_scene_status();
        let meshlets = if meshlets.is_empty() && scene.is_empty() {
            String::new()
        } else {
            format!("{meshlets}{scene}  |  ")
        };
        self.set_status(&format!(
            "{micro}{jobs}{meshlets}{progress}{play_state}{}  |  {view_name}  |  {speed}  |  {} {space}  |  Exposure: {exposure:+.1} EV  |  present={} dither={}  |  Env: {}  |  Probe: {}  |  Shadows: {shadows}{debug}{material_view}  |  {observed}  |  {}",
            self.status_base,
            self.object_tool.label(),
            self.presentation.label(),
            if self.output_dither { "on" } else { "off" },
            if self.engine.world().environment().enabled { "on" } else { "off" },
            if self.engine.world().reflection_probe_enabled() { "on" } else { "off" },
            self.probe_status()
        ));
        self.refresh_title();
    }

    fn meshlet_status(&self) -> String {
        let Some(entity) = self.selection.primary_entity() else { return String::new() };
        let Some(id) = self.engine.world().authored_mesh(entity).and_then(|(_, _, _, _, _, _, mesh, _)| match mesh {
            jarvig_core::MeshAssetRef::Asset { id, .. } => Some(id),
            _ => None,
        }) else {
            return String::new();
        };
        let Some(set) = self.engine.mesh_asset_library().meshlets(id) else { return String::new() };
        let vertices = self.engine.mesh_asset_library().get(id).map(|mesh| mesh.vertex_count()).unwrap_or(0);
        let stats = &set.stats;
        let megabytes = stats.derived_bytes as f64 / (1024.0 * 1024.0);
        let load = if stats.load_ms > 0.0 {
            format!("load {:.0} ms", stats.load_ms)
        } else {
            "load built".to_string()
        };
        format!(
            "Meshlets {} | tris {} verts {} | avg {:.1}/{:.1} min {}/{} max {}/{} | {:.1} MB | build {:.0} ms {} | colors {} | draw {}",
            stats.meshlet_count,
            stats.source_triangles,
            vertices,
            stats.average_triangles,
            stats.average_vertices,
            stats.min_triangles,
            stats.min_vertices,
            stats.max_triangles,
            stats.max_vertices,
            megabytes,
            stats.build_ms,
            load,
            if self.meshlet_debug { "on" } else { "off" },
            if self.meshlet_shade { "from meshlets" } else { "original" }
        )
    }

    fn gpu_scene_status(&self) -> String {
        let Some(renderer) = self.renderer.as_ref() else { return String::new() };
        let stats = renderer.gpu_scene_stats();
        if stats.instances == 0 && stats.meshlets == 0 {
            return String::new();
        }
        format!(
            " GPU scene inst {}/{} geom {} meshlets {}/{} fru {} occ {} cons {} tris {} cull {} us {} {}{}{}{} |",
            stats.instances_visible,
            stats.instances,
            stats.geometries,
            stats.meshlets_submitted,
            stats.meshlets,
            stats.meshlets_rejected,
            stats.occlusion_rejected,
            stats.conservative_visible,
            stats.triangles_submitted,
            stats.cull_us,
            if stats.frustum { "frustum on" } else { "frustum off" },
            if stats.occlusion { "occlusion on" } else { "occlusion off" },
            if stats.frozen { " frozen" } else { "" },
            if stats.hierarchy && stats.hierarchy_leaf_triangles > 0 {
                let percent = if stats.hierarchy_leaf_triangles == 0 {
                    0.0
                } else {
                    (stats.hierarchy_lod_triangles as f32 / stats.hierarchy_leaf_triangles as f32 - 1.0) * 100.0
                };
                format!(
                    " LOD depth {} | leaves {} | parents {} | submitted {} | tris {} / {} | {:+.1}% | select {:.2} ms | err {:.1} px",
                    stats.hierarchy_depth,
                    stats.hierarchy_leaves,
                    stats.hierarchy_parents,
                    stats.hierarchy_leaves.saturating_add(stats.hierarchy_parents),
                    stats.hierarchy_lod_triangles,
                    stats.hierarchy_leaf_triangles,
                    percent,
                    stats.hierarchy_select_us as f32 / 1000.0,
                    self.hierarchy_error_px
                )
            } else if stats.hierarchy {
                format!(" hier {}/{}", stats.hierarchy_cut, stats.hierarchy_nodes)
            } else {
                String::new()
            },
            self.einstein_status()
        )
    }

    fn remember_scene_meshlets(&mut self, snapshot: &jarvig_core::RenderSceneSnapshot) {
        let mut fresh = Vec::new();
        for instance in snapshot.instances() {
            if self.scene_meshes.contains(&instance.mesh) {
                continue;
            }
            self.scene_meshes.push(instance.mesh);
            fresh.push((instance.mesh, self.engine.meshlet_records(instance.entity)));
        }
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_meshlet_frustum(self.meshlet_frustum);
            renderer.set_meshlet_occlusion(self.meshlet_occlusion);
            renderer.set_meshlet_freeze(self.meshlet_freeze);
            renderer.set_cluster_hierarchy(self.cluster_hierarchy);
            renderer.set_hierarchy_error_px(self.hierarchy_error_px);
        }
        if fresh.is_empty() {
            return;
        }
        if let Some(renderer) = self.renderer.as_mut() {
            for (mesh, records) in &fresh {
                renderer.remember_meshlets(*mesh, records);
            }
            let stats = renderer.gpu_scene_stats();
            if fresh.iter().any(|(_, records)| !records.is_empty()) {
                self.append(&format!(
                    "JRV-0025 GPU scene remembered {} meshlet descriptors across {} new meshes. Instances stay separate. The ordinary draw is unchanged.",
                    fresh.iter().map(|(_, records)| records.len()).sum::<usize>(),
                    fresh.len()
                ));
            }
            let _ = stats;
        }
    }

    fn note_loaded_meshlets(&mut self, library: &jarvig_core::MeshAssetLibrary) {
        for record in library.records() {
            let Some(set) = library.meshlets(record.id) else { continue };
            let stats = &set.stats;
            let megabytes = stats.derived_bytes as f64 / (1024.0 * 1024.0);
            let load = if stats.load_ms > 0.0 {
                format!("load {:.0} ms", stats.load_ms)
            } else {
                "built this launch".to_string()
            };
            self.append(&format!(
                "JRV-0028 {} meshlets={} triangles={} avg_tris={:.2} min_tris={} max_tris={} avg_verts={:.2} min_verts={} max_verts={} bytes={} ({:.1} MB) build_ms={:.1} {load}. View > Show Meshlet Colors paints one patch per cluster. View > Draw From Meshlets uses those clusters with the normal materials. Both stay off until checked.",
                record.name,
                stats.meshlet_count,
                stats.source_triangles,
                stats.average_triangles,
                stats.min_triangles,
                stats.max_triangles,
                stats.average_vertices,
                stats.min_vertices,
                stats.max_vertices,
                stats.derived_bytes,
                megabytes,
                stats.build_ms
            ));
        }
    }

    fn probe_status(&self) -> String {
        let Some(renderer) = self.renderer.as_ref() else {
            return "capture pending".into();
        };
        let policy = match self.engine.world().probe_update_policy() {
            jarvig_core::ProbeUpdatePolicy::Static => "static",
            jarvig_core::ProbeUpdatePolicy::OnDemand => "on-demand",
            jarvig_core::ProbeUpdatePolicy::OnTransformChange => "on-transform",
            jarvig_core::ProbeUpdatePolicy::OnLightingChange => "on-lighting",
            jarvig_core::ProbeUpdatePolicy::TimeSliced => "time-sliced",
        };
        let resolution = renderer.reflection_probe_resolution();
        let mips = renderer.reflection_probe_mip_count().max(1);
        // Smooth Sphere roughness is the perceptual minimum, 0.045. Flat Sphere uses the same value and is not smoothed here.
        let selected_lod = jarvig_core::reflection_probe_lod(0.045, mips);
        let weight = self.smooth_sphere_probe_weight();
        let pending = renderer.probe_capture_job_resolution();
        let pending_text = if renderer.probe_capture_job_steps() > 0 {
            format!(" pending {pending} {}/{}", renderer.probe_capture_job_step(), renderer.probe_capture_job_steps())
        } else {
            String::new()
        };
        let spheres = if self.engine.world().entity_outline().iter().any(|row| row.name == "Metal Sphere")
            && self.engine.world().entity_outline().iter().any(|row| row.name == "Flat Sphere")
        {
            "sphere=smooth flat=face"
        } else {
            "sphere=bootstrap"
        };
        format!(
            "quality={} {policy} capture_resolution={resolution} gpu_budget={}{pending_text} capture_ms={} prefilter_ms={} mip_count={mips} mirror_lod={selected_lod:.2} probe_weight={weight:.2} queue={} budget=1 cap={} recap={} {}KB irr=cosine output=dither seam=cube-sample {spheres} shadow=pcss+cascades",
            self.render_quality.label(),
            self.probe_budget,
            renderer.last_probe_capture_ms(),
            renderer.last_probe_prefilter_ms(),
            renderer.probe_capture_queue_len(),
            renderer.reflection_probe_capture_count(),
            renderer.probe_recapture_count(),
            renderer.reflection_probe_memory_bytes() / 1024,
        )
    }

    fn set_render_quality(&mut self, quality: jarvig_core::RenderQuality) {
        let budget = quality.budget();
        self.render_quality = quality;
        self.probe_budget = budget.probe_resolution;
        self.append(&format!(
            "Renderer quality is {}. Probe budget {} px. Shadow atlas stays {} px and the filter stays {} taps. GI updates {} and ray budget {} are unused. The open level was not rewritten.",
            quality.label(),
            budget.probe_resolution,
            budget.shadow_resolution,
            budget.shadow_filter_taps,
            budget.gi_updates_per_frame,
            budget.reflection_ray_budget
        ));
        self.sync_view_menu();
        self.refresh_status();
    }

    fn set_probe_capture_resolution(&mut self, resolution: u32) {
        if self.engine.world_mut().set_reflection_probe_resolution(resolution).is_err() {
            self.append(&format!("Probe resolution {resolution} was rejected."));
            return;
        }
        self.append(&format!(
            "Probe capture set to {resolution}. Mip 0 stays sharp. The cube on screen stays until the update finishes. Renderer quality {} budgets {} px and does not replace this capture by itself.",
            self.render_quality.label(),
            self.probe_budget
        ));
        self.refresh_status();
    }

    /// Weight of the smooth metal sphere, the mirror used to judge capture detail. Flat Sphere is not this.
    fn smooth_sphere_probe_weight(&self) -> f32 {
        let world = self.engine.world();
        let Some(row) = world.entity_outline().into_iter().find(|row| row.name == "Metal Sphere") else {
            return 0.0;
        };
        let Ok(pose) = world.entity_world_pose(row.uuid) else {
            return 0.0;
        };
        let Ok(snapshot) = world.extract(RenderFrameId(1)) else {
            return 0.0;
        };
        let Some(probe) = jarvig_core::select_reflection_probe(snapshot.reflection_probes()) else {
            return 0.0;
        };
        jarvig_core::reflection_probe_weight(probe, pose.translation)
    }

    fn set_exposure_ev(&mut self, ev: f32) -> Result<(), String> {
        let ev = ev.clamp(jarvig_renderer::EXPOSURE_EV_MIN, jarvig_renderer::EXPOSURE_EV_MAX);
        let lighting = self.lighting_debug;
        let settings = self.view_settings(ev, lighting);
        let view = self.viewport_view.ok_or("viewport view missing")?;
        let renderer = self.renderer.as_mut().ok_or("renderer missing")?;
        renderer
            .update_view(
                view,
                RenderViewUpdate {
                    camera: None,
                    layout: None,
                    settings: Some(settings),
                    pose: None,
                },
            )
            .map_err(|error| error.to_string())?;
        self.refresh_status();
        Ok(())
    }

    fn nudge_exposure(&mut self, stops: f32) -> Result<(), String> {
        let current = self
            .viewport_view
            .and_then(|view| self.renderer.as_ref().and_then(|renderer| renderer.exposure_ev(view).ok()))
            .unwrap_or(0.0);
        self.set_exposure_ev(current + stops)
    }

    fn advance_local_probe(&mut self) -> Result<(), String> {
        self.assert_drawable()?;
        match self.local_probe {
            0 => {
                let (meshes, textures, captures) = {
                    let renderer = self.renderer.as_ref().ok_or("renderer missing")?;
                    if renderer.reflection_probe_capture_count() != 1
                        || renderer.reflection_probe_resolution() != 32
                        || renderer.reflection_probe_mip_count() != REFLECTION_PROBE_MIP_COUNT
                    {
                        return Err(format!(
                            "probe capture was not ready count={} res={} mips={}",
                            renderer.reflection_probe_capture_count(),
                            renderer.reflection_probe_resolution(),
                            renderer.reflection_probe_mip_count()
                        ));
                    }
                    (renderer.mesh_upload_count(), renderer.texture_upload_count(), renderer.reflection_probe_capture_count())
                };
                let weight = self.far_probe_weight()?;
                if weight <= 0.5 {
                    return Err(format!("far metal started outside the probe ({weight})"));
                }
                let far = self.far_entity()?;
                self.local_weight = weight;
                self.local_translation = self.local_translation_of(far)?;
                self.local_rotation = self.local_rotation_of(far)?;
                let camera = self.editor_camera.as_ref().ok_or("camera missing")?;
                self.local_camera = camera.position;
                self.local_yaw = camera.yaw;
                self.local_compiles = self.engine.material_compile_count();
                self.local_meshes = meshes;
                self.local_textures = textures;
                self.local_captures = captures;
                self.local_revision = self.engine.world().revision();
                self.local_probe = 1;
                if let Some(camera) = self.editor_camera.as_mut() {
                    camera.position.y += 1.5;
                    camera.yaw += 0.4;
                }
                Ok(())
            }
            1 => {
                self.require_local_resources()?;
                if self.engine.world().revision() != self.local_revision {
                    return Err("camera motion revised the world during the probe test".into());
                }
                let weight = self.far_probe_weight()?;
                if (weight - self.local_weight).abs() > 1.0e-4 {
                    return Err(format!("camera motion changed probe weight {weight} from {}", self.local_weight));
                }
                if let Some(camera) = self.editor_camera.as_mut() {
                    camera.position = self.local_camera;
                    camera.yaw = self.local_yaw;
                }
                self.local_probe = 2;
                self.set_far_translation(Vec3::new(0.0, 0.0, -30.0))?;
                Ok(())
            }
            2 => {
                self.require_local_resources()?;
                let weight = self.far_probe_weight()?;
                if weight != 0.0 {
                    return Err(format!("metal outside the radius still selected the probe ({weight})"));
                }
                let renderer = self.renderer.as_ref().ok_or("renderer missing")?;
                if renderer.reflection_probe_selected_far() != 0 || renderer.reflection_probe_fallback_count() == 0 {
                    return Err(format!(
                        "fallback diagnostics selected={} fallback={}",
                        renderer.reflection_probe_selected_far(),
                        renderer.reflection_probe_fallback_count()
                    ));
                }
                self.local_probe = 3;
                self.set_far_translation(self.local_translation)?;
                Ok(())
            }
            3 => {
                self.require_local_resources()?;
                let weight = self.far_probe_weight()?;
                if weight <= 0.5 {
                    return Err(format!("restoring the metal did not reselect the probe ({weight})"));
                }
                self.local_probe = 4;
                self.set_far_rotation(rotation_about_x(std::f64::consts::PI))?;
                Ok(())
            }
            4 => {
                self.require_local_resources()?;
                let entity = self.far_entity()?;
                let geometric = plus_z_normal(self.engine.world().entity_world_pose(entity).map_err(|error| error.to_string())?.rotation);
                let view = self.direction_to_camera(entity)?;
                let facing = geometric[0] * view[0] + geometric[1] * view[1] + geometric[2] * view[2];
                if facing >= 0.0 {
                    return Err(format!("probe reverse side is still front-facing ({facing})"));
                }
                let visible = self.far_reflection(0.2)?;
                if visible.iter().all(|channel| *channel <= 0.002) {
                    return Err(format!("two-sided reflection was {visible:?}"));
                }
                if self.far_probe_weight()? <= 0.5 {
                    return Err("turning the metal dropped it out of the probe".into());
                }
                self.set_far_rotation(self.local_rotation)?;
                let id = self.engine.world().reflection_probe_ids().next().ok_or("probe missing")?;
                self.engine.world_mut().set_reflection_probe_enabled(id, false).map_err(|error| error.to_string())?;
                self.local_probe = 5;
                Ok(())
            }
            5 => {
                self.require_local_resources()?;
                if self.far_probe_weight()? != 0.0 || self.engine.world().reflection_probe_enabled() {
                    return Err("disabling the probe did not fall back".into());
                }
                let id = self.engine.world().reflection_probe_ids().next().ok_or("probe missing")?;
                self.engine.world_mut().set_reflection_probe_enabled(id, true).map_err(|error| error.to_string())?;
                self.local_probe = 6;
                Ok(())
            }
            _ => {
                self.require_local_resources()?;
                if self.far_probe_weight()? <= 0.5 || !self.engine.world().reflection_probe_enabled() {
                    return Err("re-enabling the probe did not select it".into());
                }
                let sharp = reflection_probe_lod(0.1, REFLECTION_PROBE_MIP_COUNT);
                let rough = reflection_probe_lod(0.9, REFLECTION_PROBE_MIP_COUNT);
                if !(sharp < rough) {
                    return Err(format!("roughness did not select a broader mip ({sharp} vs {rough})"));
                }
                self.local_probe = 7;
                self.local_report = format!(
                    "JRV-0074 probe inside=PASS outside=PASS restore=PASS camera=PASS reverse=PASS disabled=PASS roughness_lod={sharp:.2}->{rough:.2} capture_count={}",
                    self.local_captures
                );
                Ok(())
            }
        }
    }

    fn require_local_resources(&self) -> Result<(), String> {
        let renderer = self.renderer.as_ref().ok_or("renderer missing")?;
        if self.engine.material_compile_count() != self.local_compiles
            || renderer.mesh_upload_count() != self.local_meshes
            || renderer.texture_upload_count() != self.local_textures
            || renderer.reflection_probe_capture_count() != self.local_captures
        {
            return Err(format!(
                "probe influence rebuilt resources compiles={} meshes={} textures={} captures={}",
                self.engine.material_compile_count(),
                renderer.mesh_upload_count(),
                renderer.texture_upload_count(),
                renderer.reflection_probe_capture_count()
            ));
        }
        Ok(())
    }

    fn far_probe_weight(&self) -> Result<f32, String> {
        let entity = self.far_entity()?;
        let pose = self.engine.world().entity_world_pose(entity).map_err(|error| error.to_string())?;
        let snapshot = self.engine.world().extract(RenderFrameId(1)).map_err(|error| error.to_string())?;
        Ok(reflection_probe_influence(snapshot.reflection_probes(), pose.translation))
    }

    fn set_far_translation(&mut self, translation: Vec3) -> Result<(), String> {
        let far = self.far_entity()?;
        self.engine
            .execute_authoring(AuthoringCommand::SetProperty {
                target: far,
                type_id: TYPE_SPATIAL_FRAME,
                field: jarvig_core::FIELD_LOCAL_TRANSLATION,
                value: PropertyValue::Vec3(translation),
            })
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn advance_spec_probe(&mut self) -> Result<(), String> {
        self.assert_drawable()?;
        match self.spec_probe {
            0 => {
                let far = self.far_entity()?;
                self.spec_rotation = self.local_rotation_of(far)?;
                let camera = self.editor_camera.as_ref().ok_or("camera missing")?;
                self.spec_camera = camera.position;
                self.spec_yaw = camera.yaw;
                self.spec_updates = 0;
                self.spec_probe = 1;
                self.set_direct_lights_enabled(false)?;
                self.engine
                    .set_material_float(self.far_material()?, "RoughnessFactor", 0.1)
                    .map_err(|error| error.to_string())?;
                Ok(())
            }
            1 => {
                self.require_env_resources()?;
                if self.engine.last_light_count() != 0 {
                    return Err("specular probe still had direct lights".into());
                }
                let sharp = self.far_reflection(0.1)?;
                if sharp.iter().all(|channel| *channel <= 0.002) {
                    return Err(format!("metal environment specular was {sharp:?}"));
                }
                let diffuse = self.far_diffuse()?;
                if diffuse.iter().any(|channel| channel.abs() > 1.0e-4) {
                    return Err(format!("metal still received environment diffuse {diffuse:?}"));
                }
                let entity = self.far_entity()?;
                let view = self.direction_to_camera(entity)?;
                let geometric = plus_z_normal(self.engine.world().entity_world_pose(entity).map_err(|error| error.to_string())?.rotation);
                let normal = visible_side_normal(geometric, true);
                let glass = environment_specular_for(&self.engine.world().environment(), normal, view, [1.0, 1.0, 1.0], 0.0, 0.1);
                let metal_sum = sharp[0] + sharp[1] + sharp[2];
                let glass_sum = glass[0] + glass[1] + glass[2];
                if glass_sum <= 0.0 || metal_sum <= glass_sum {
                    return Err(format!("dielectric reflection {glass:?} was not smaller than the metal {sharp:?}"));
                }
                self.spec_low = sharp;
                self.spec_revision = self.engine.world().revision();
                self.spec_probe = 2;
                if let Some(camera) = self.editor_camera.as_mut() {
                    camera.position.y += 3.0;
                    camera.yaw += 0.7;
                }
                Ok(())
            }
            2 => {
                self.require_env_resources()?;
                if self.engine.world().revision() != self.spec_revision {
                    return Err("camera motion revised the world".into());
                }
                let moved = self.far_reflection(0.1)?;
                if rgb_close(moved, self.spec_low) {
                    return Err(format!("camera motion did not change the reflection {moved:?}"));
                }
                self.spec_updates = self.spec_updates.saturating_add(1);
                self.spec_revision = self.engine.world().revision();
                self.spec_probe = 3;
                self.set_far_rotation(rotation_about_x(-1.2))?;
                Ok(())
            }
            3 => {
                self.require_env_resources()?;
                if self.engine.world().revision() == self.spec_revision {
                    return Err("rotating the metal did not revise the world".into());
                }
                let turned = self.far_reflection(0.1)?;
                if rgb_close(turned, self.spec_low) {
                    return Err(format!("rotation did not change the reflection {turned:?}"));
                }
                self.spec_updates = self.spec_updates.saturating_add(1);
                self.spec_low = turned;
                self.spec_probe = 4;
                self.engine
                    .set_material_float(self.far_material()?, "RoughnessFactor", 0.9)
                    .map_err(|error| error.to_string())?;
                Ok(())
            }
            4 => {
                self.require_env_resources()?;
                let rough = self.far_reflection(0.9)?;
                let sharp_here = self.far_reflection(0.1)?;
                if rgb_close(rough, sharp_here) || rgb_close(chroma(rough), chroma(sharp_here)) {
                    return Err(format!("roughness did not change the reflection character {rough:?} vs {sharp_here:?}"));
                }
                self.spec_updates = self.spec_updates.saturating_add(1);
                self.spec_probe = 5;
                self.set_far_rotation(rotation_about_x(std::f64::consts::PI))?;
                Ok(())
            }
            5 => {
                self.require_env_resources()?;
                let entity = self.far_entity()?;
                let geometric = plus_z_normal(self.engine.world().entity_world_pose(entity).map_err(|error| error.to_string())?.rotation);
                let view = self.direction_to_camera(entity)?;
                let facing = geometric[0] * view[0] + geometric[1] * view[1] + geometric[2] * view[2];
                if facing >= 0.0 {
                    return Err(format!("reverse side is still front-facing ({facing})"));
                }
                let stale = environment_specular_for(&self.engine.world().environment(), geometric, view, [1.0, 1.0, 1.0], 1.0, 0.9);
                if stale != [0.0; 3] {
                    return Err("unflipped back normal still reflected".into());
                }
                let visible = self.far_reflection(0.9)?;
                if visible.iter().any(|channel| !channel.is_finite()) || visible.iter().all(|channel| *channel <= 0.002) {
                    return Err(format!("two-sided reflection was {visible:?}"));
                }
                self.spec_updates = self.spec_updates.saturating_add(1);
                self.spec_probe = 6;
                self.engine.world_mut().set_environment_enabled(false).map_err(|error| error.to_string())?;
                Ok(())
            }
            6 => {
                self.require_env_resources()?;
                let dark = self.far_reflection(0.9)?;
                if dark != [0.0; 3] {
                    return Err(format!("disabled environment still reflected {dark:?}"));
                }
                self.restore_spec_scene()?;
                self.spec_probe = 7;
                Ok(())
            }
            7 => {
                self.require_env_resources()?;
                if self.engine.last_light_count() != 3 || !self.engine.world().environment().enabled {
                    return Err("specular restore did not bring back direct lights and the environment".into());
                }
                self.spec_revision = self.engine.world().revision();
                self.spec_probe = 8;
                self.set_exposure_ev(2.0)?;
                Ok(())
            }
            8 => {
                self.require_env_resources()?;
                self.exposure_is(2.0)?;
                if self.engine.world().revision() != self.spec_revision {
                    return Err("exposure revised the world during the specular probe".into());
                }
                self.spec_probe = 9;
                self.set_exposure_ev(0.0)?;
                Ok(())
            }
            _ => {
                self.require_env_resources()?;
                self.exposure_is(0.0)?;
                self.spec_probe = 10;
                self.spec_report = format!(
                    "JRV-0073 specular rotation=PASS camera=PASS roughness=PASS metallic=PASS dielectric_model=PASS reverse=PASS disabled=PASS exposure=PASS environment_reflection_updates={}",
                    self.spec_updates
                );
                Ok(())
            }
        }
    }

    fn restore_spec_scene(&mut self) -> Result<(), String> {
        self.set_far_rotation(self.spec_rotation)?;
        self.engine
            .set_material_float(self.far_material()?, "RoughnessFactor", 0.2)
            .map_err(|error| error.to_string())?;
        self.set_direct_lights_enabled(true)?;
        self.engine.world_mut().set_environment_enabled(true).map_err(|error| error.to_string())?;
        if let Some(camera) = self.editor_camera.as_mut() {
            camera.position = self.spec_camera;
            camera.yaw = self.spec_yaw;
        }
        Ok(())
    }

    fn far_entity(&self) -> Result<EntityUuid, String> {
        self.engine.world().entity_outline().get(1).map(|row| row.uuid).ok_or_else(|| "far entity missing".into())
    }

    fn far_material(&self) -> Result<jarvig_core::MaterialInstanceId, String> {
        let snapshot = self.engine.world().extract(RenderFrameId(1)).map_err(|error| error.to_string())?;
        snapshot.instances().get(1).and_then(|instance| instance.material_for_slot(0)).ok_or_else(|| "far material missing".into())
    }

    fn direction_to_camera(&self, entity: EntityUuid) -> Result<[f32; 3], String> {
        let pose = self.engine.world().entity_world_pose(entity).map_err(|error| error.to_string())?;
        let camera = self.editor_camera.as_ref().ok_or("camera missing")?.position;
        let delta = [
            (camera.x - pose.translation.x) as f32,
            (camera.y - pose.translation.y) as f32,
            (camera.z - pose.translation.z) as f32,
        ];
        let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
        if length < 1.0e-5 {
            return Err("camera is on the metal".into());
        }
        Ok([delta[0] / length, delta[1] / length, delta[2] / length])
    }

    fn far_reflection(&self, roughness: f32) -> Result<[f32; 3], String> {
        let entity = self.far_entity()?;
        let geometric = plus_z_normal(self.engine.world().entity_world_pose(entity).map_err(|error| error.to_string())?.rotation);
        let view = self.direction_to_camera(entity)?;
        let facing = geometric[0] * view[0] + geometric[1] * view[1] + geometric[2] * view[2];
        let normal = visible_side_normal(geometric, facing > 0.0);
        Ok(environment_specular_for(&self.engine.world().environment(), normal, view, [1.0, 1.0, 1.0], 1.0, roughness))
    }

    fn far_diffuse(&self) -> Result<[f32; 3], String> {
        let entity = self.far_entity()?;
        let geometric = plus_z_normal(self.engine.world().entity_world_pose(entity).map_err(|error| error.to_string())?.rotation);
        let view = self.direction_to_camera(entity)?;
        let facing = geometric[0] * view[0] + geometric[1] * view[1] + geometric[2] * view[2];
        let normal = visible_side_normal(geometric, facing > 0.0);
        Ok(environment_diffuse_for(&self.engine.world().environment(), normal, [1.0, 1.0, 1.0], 1.0, 1.0))
    }

    fn set_far_rotation(&mut self, rotation: Quat) -> Result<(), String> {
        let far = self.far_entity()?;
        self.engine
            .execute_authoring(AuthoringCommand::SetProperty {
                target: far,
                type_id: TYPE_SPATIAL_FRAME,
                field: FIELD_LOCAL_ROTATION,
                value: PropertyValue::Quat(rotation),
            })
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn advance_env_probe(&mut self) -> Result<(), String> {
        self.assert_drawable()?;
        let normal = self.near_normal()?;
        match self.env_probe {
            0 => {
                let (meshes, textures, tone, uploads, packets) = {
                    let renderer = self.renderer.as_ref().ok_or("renderer missing")?;
                    (
                        renderer.mesh_upload_count(),
                        renderer.texture_upload_count(),
                        renderer.tone_map_pipeline_count(),
                        renderer.instance_transform_upload_count(),
                        renderer.environment_packet_upload_count(),
                    )
                };
                self.env_compiles = self.engine.material_compile_count();
                self.env_meshes = meshes;
                self.env_textures = textures;
                self.env_tone = tone;
                self.env_uploads = uploads;
                self.env_packets = packets;
                let near = self.near_entity()?;
                self.env_rotation = self.local_rotation_of(near)?;
                self.env_base = self.unit_environment(normal, 0.0, 1.0);
                self.env_probe = 1;
                self.set_direct_lights_enabled(false)?;
                self.set_near_rotation(rotation_about_x(-std::f64::consts::FRAC_PI_2))?;
                Ok(())
            }
            1 => {
                self.require_env_resources()?;
                if normal[1] < 0.99 {
                    return Err(format!("upward normal was {normal:?}"));
                }
                let up = self.unit_environment(normal, 0.0, 1.0);
                if up[2] <= up[0] || up[1] < 0.05 {
                    return Err(format!("upward environment was not the cool sky {up:?}"));
                }
                if self.unit_environment(normal, 0.0, 0.0) != [0.0; 3] {
                    return Err("ao 0 did not remove the environment term".into());
                }
                if self.engine.last_light_count() != 0 {
                    return Err(format!("direct lights were still extracted ({})", self.engine.last_light_count()));
                }
                let uploaded = self.renderer.as_ref().ok_or("renderer missing")?.instance_transform_upload_count();
                let packets = self.renderer.as_ref().ok_or("renderer missing")?.environment_packet_upload_count();
                if uploaded <= self.env_uploads {
                    return Err("the upward rotation was not uploaded before the next present".into());
                }
                if packets != self.env_packets {
                    return Err("rotating the triangle rewrote the environment packet".into());
                }
                let _ = capture_window(self.frame, SHOT_ENVIRONMENT);
                self.env_up = up;
                self.env_probe = 2;
                self.set_near_rotation(rotation_about_x(-std::f64::consts::PI))?;
                Ok(())
            }
            2 => {
                self.require_env_resources()?;
                if normal[1].abs() > 0.05 {
                    return Err(format!("90 degree normal was not level {normal:?}"));
                }
                let side = self.unit_environment(normal, 0.0, 1.0);
                if !rgb_close(side, self.env_base) || side[1] >= self.env_up[1] {
                    return Err(format!("90 degree environment {side:?} did not leave the upper sky {:?}", self.env_up));
                }
                self.env_probe = 3;
                self.set_near_rotation(rotation_about_x(std::f64::consts::FRAC_PI_2))?;
                Ok(())
            }
            3 => {
                self.require_env_resources()?;
                if normal[1] > -0.99 {
                    return Err(format!("downward normal was {normal:?}"));
                }
                let down = self.unit_environment(normal, 0.0, 1.0);
                if down[0] <= down[2] || down[1] >= self.env_base[1] {
                    return Err(format!("180 degree environment was not the lower hemisphere {down:?}"));
                }
                self.env_probe = 4;
                self.set_near_rotation(self.env_rotation)?;
                Ok(())
            }
            4 => {
                self.require_env_resources()?;
                let restored = self.unit_environment(normal, 0.0, 1.0);
                if !rgb_close(restored, self.env_base) {
                    return Err(format!("restored environment {restored:?} != baseline {:?}", self.env_base));
                }
                let material = self.near_material()?;
                self.engine.set_material_float(material, "MetallicFactor", 1.0).map_err(|error| error.to_string())?;
                self.env_probe = 5;
                Ok(())
            }
            5 => {
                self.require_env_resources()?;
                let metal = self.unit_environment(normal, 1.0, 1.0);
                if metal.iter().any(|channel| channel.abs() > 1.0e-5) {
                    return Err(format!("metallic environment was {metal:?}"));
                }
                let material = self.near_material()?;
                self.engine.set_material_float(material, "MetallicFactor", 0.0).map_err(|error| error.to_string())?;
                self.set_direct_lights_enabled(true)?;
                self.env_probe = 6;
                Ok(())
            }
            6 => {
                self.require_env_resources()?;
                if self.engine.last_light_count() != 3 {
                    return Err(format!("direct lights did not return ({})", self.engine.last_light_count()));
                }
                let both = self.unit_environment(normal, 0.0, 1.0);
                if !rgb_close(both, self.env_base) {
                    return Err(format!("direct plus environment lost the diffuse term {both:?}"));
                }
                self.engine.world_mut().set_environment_enabled(false).map_err(|error| error.to_string())?;
                self.env_probe = 7;
                Ok(())
            }
            7 => {
                self.require_env_resources()?;
                if self.engine.last_light_count() != 3 || self.engine.world().environment().enabled {
                    return Err("disabled environment did not keep the direct lights".into());
                }
                if self.unit_environment(normal, 0.0, 1.0) != [0.0; 3] {
                    return Err("disabled environment still contributed".into());
                }
                self.engine.world_mut().set_environment_enabled(true).map_err(|error| error.to_string())?;
                self.engine.world_mut().set_environment_intensity(0.05).map_err(|error| error.to_string())?;
                self.env_probe = 8;
                Ok(())
            }
            8 => {
                self.require_env_resources()?;
                let packets = self.renderer.as_ref().ok_or("renderer missing")?.environment_packet_upload_count();
                if packets <= self.env_packets {
                    return Err("environment edits did not upload a packet".into());
                }
                self.engine
                    .world_mut()
                    .set_environment_intensity(jarvig_core::BOOTSTRAP_ENVIRONMENT_INTENSITY)
                    .map_err(|error| error.to_string())?;
                self.env_exposure_revision = self.engine.world().revision();
                self.env_probe = 9;
                self.set_exposure_ev(2.0)?;
                Ok(())
            }
            9 => {
                self.require_env_resources()?;
                self.require_env_exposure(2.0)?;
                self.env_probe = 10;
                self.set_exposure_ev(-2.0)?;
                Ok(())
            }
            _ => {
                self.require_env_resources()?;
                self.require_env_exposure(-2.0)?;
                self.env_probe = 11;
                self.env_report = "JRV-0072 environment rotation=PASS rotate_back=PASS direct_plus_environment=PASS disabled=PASS metallic=PASS ao=PASS resource_regression=PASS live_authoring=PASS".into();
                self.set_exposure_ev(0.0)?;
                Ok(())
            }
        }
    }

    fn require_env_resources(&self) -> Result<(), String> {
        if self.engine.material_compile_count() != self.env_compiles {
            return Err("environment work recompiled a material".into());
        }
        let renderer = self.renderer.as_ref().ok_or("renderer missing")?;
        if renderer.mesh_upload_count() != self.env_meshes || renderer.texture_upload_count() != self.env_textures {
            return Err("environment work reuploaded a mesh or a texture".into());
        }
        if renderer.tone_map_pipeline_count() != self.env_tone {
            return Err("environment work rebuilt the tone-map pipeline".into());
        }
        Ok(())
    }

    fn require_env_exposure(&self, ev: f32) -> Result<(), String> {
        if self.engine.world().revision() != self.env_exposure_revision {
            return Err("exposure revised the world during the environment probe".into());
        }
        self.exposure_is(ev)
    }

    fn exposure_is(&self, ev: f32) -> Result<(), String> {
        let actual = self
            .viewport_view
            .and_then(|view| self.renderer.as_ref().and_then(|renderer| renderer.exposure_ev(view).ok()))
            .unwrap_or(99.0);
        if (actual - ev).abs() > 1.0e-4 {
            return Err(format!("exposure was {actual}, expected {ev}"));
        }
        Ok(())
    }

    fn near_entity(&self) -> Result<EntityUuid, String> {
        self.engine.world().entity_outline().first().map(|row| row.uuid).ok_or_else(|| "near entity missing".into())
    }

    fn near_material(&self) -> Result<jarvig_core::MaterialInstanceId, String> {
        let snapshot = self.engine.world().extract(RenderFrameId(1)).map_err(|error| error.to_string())?;
        snapshot.instances().first().and_then(|instance| instance.material_for_slot(0)).ok_or_else(|| "near material missing".into())
    }

    fn near_normal(&self) -> Result<[f32; 3], String> {
        let pose = self.engine.world().entity_world_pose(self.near_entity()?).map_err(|error| error.to_string())?;
        Ok(plus_z_normal(pose.rotation))
    }

    fn unit_environment(&self, normal: [f32; 3], metallic: f32, ambient_occlusion: f32) -> [f32; 3] {
        environment_diffuse_for(&self.engine.world().environment(), normal, [1.0, 1.0, 1.0], metallic, ambient_occlusion)
    }

    fn set_near_rotation(&mut self, rotation: Quat) -> Result<(), String> {
        let near = self.near_entity()?;
        self.engine
            .execute_authoring(AuthoringCommand::SetProperty {
                target: near,
                type_id: TYPE_SPATIAL_FRAME,
                field: FIELD_LOCAL_ROTATION,
                value: PropertyValue::Quat(rotation),
            })
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn set_direct_lights_enabled(&mut self, enabled: bool) -> Result<(), String> {
        let ids: Vec<_> = self.engine.world().light_ids().collect();
        for id in ids {
            self.engine.world_mut().set_light_enabled(id, enabled).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn advance_hdr_probe(&mut self) -> Result<(), String> {
        self.assert_drawable()?;
        let renderer = self.renderer.as_ref().ok_or("renderer missing")?;
        if renderer.hdr_scene_format() != "rgba16float" || renderer.hdr_scene_target_count() != 1 || renderer.tone_map_pipeline_count() != 1 {
            return Err(format!(
                "hdr format {} count {} pipelines {}",
                renderer.hdr_scene_format(),
                renderer.hdr_scene_target_count(),
                renderer.tone_map_pipeline_count()
            ));
        }
        match self.hdr_probe {
            0 => {
                self.hdr_world = self.engine.world().revision();
                self.hdr_selection = self.selection.revision();
                self.hdr_compiles = self.engine.material_compile_count();
                self.hdr_meshes = renderer.mesh_upload_count();
                self.hdr_textures = renderer.texture_upload_count();
                self.hdr_pipelines = renderer.tone_map_pipeline_count();
                self.hdr_probe = 1;
                self.set_exposure_ev(2.0)
            }
            1 | 2 => {
                self.require_exposure_invariant()?;
                let next = if self.hdr_probe == 1 { -2.0 } else { 0.0 };
                self.hdr_probe += 1;
                self.set_exposure_ev(next)
            }
            _ => {
                self.require_exposure_invariant()?;
                self.hdr_probe = 4;
                self.set_exposure_ev(0.0)
            }
        }
    }

    fn require_exposure_invariant(&self) -> Result<(), String> {
        if self.engine.world().revision() != self.hdr_world {
            return Err("exposure revised the world".into());
        }
        if self.selection.revision() != self.hdr_selection {
            return Err("exposure changed the selection".into());
        }
        if self.engine.material_compile_count() != self.hdr_compiles {
            return Err("exposure recompiled a material".into());
        }
        let renderer = self.renderer.as_ref().ok_or("renderer missing")?;
        if renderer.mesh_upload_count() != self.hdr_meshes || renderer.texture_upload_count() != self.hdr_textures {
            return Err("exposure reuploaded a mesh or a texture".into());
        }
        if renderer.tone_map_pipeline_count() != self.hdr_pipelines {
            return Err("exposure rebuilt the tone-map pipeline".into());
        }
        Ok(())
    }

    fn navigation_open(&self) -> bool {
        camera::navigation_allowed(
            self.workspace.focused() == Some(PERSPECTIVE) || self.capture.is_some(),
            self.text_focused,
            self.perspective_visible() && unsafe { IsIconic(self.frame) == 0 },
        )
    }

    fn tab_title(&self, panel: PanelId) -> String {
        if self.character_workspace && !self.session_active() {
            if panel == OUTLINER {
                return "Skeleton".into();
            }
            if panel == PERSPECTIVE {
                return "Character Preview".into();
            }
        }
        if self.land_mode && !self.session_active() && panel == OUTLINER {
            return "Land".into();
        }
        if panel == PERSPECTIVE {
            return match self.play {
                PlayPhase::Playing | PlayPhase::StartingPlay => "Perspective  PLAY".into(),
                PlayPhase::Paused => "Perspective  PAUSED".into(),
                PlayPhase::StoppingPlay | PlayPhase::Editing => self.workspace.title(panel).to_string(),
            };
        }
        self.workspace.title(panel).to_string()
    }

    fn play_toolbar(&self) -> Option<chrome::ToolbarCommand> {
        match self.play {
            PlayPhase::Playing | PlayPhase::StartingPlay => Some(chrome::ToolbarCommand::Play),
            PlayPhase::Paused => Some(chrome::ToolbarCommand::Pause),
            PlayPhase::StoppingPlay | PlayPhase::Editing => None,
        }
    }

    fn render_runtime(&mut self, target: jarvig_renderer::RenderTargetId, delta: f64, outcome: &mut FrameOutcome) -> Result<bool, String> {
        if self.play == PlayPhase::Playing {
            self.sample_play_mouse();
            let wants_capture = if let Ok(world) = self.game.runtime_world_mut() {
                let _ = self.play_control.tick(world, delta);
                self.play_control.wants_capture()
            } else {
                false
            };
            if wants_capture && !self.play_captured {
                self.begin_play_capture();
            } else if !wants_capture && self.play_captured {
                self.end_play_capture();
            }
            self.game.tick(delta).map_err(|error| error.to_string())?;
        }
        let snapshot = self.game.extract_snapshot().map_err(|error| error.to_string())?;
        self.remember_scene_meshlets(&snapshot);
        self.push_runtime_camera(&snapshot)?;
        let meshes = self.game.runtime_world().ok_or("runtime world missing")?.meshes() as *const _;
        let materials = self.engine.materials() as *const _;
        let textures = self.engine.textures() as *const _;
        let renderer = self.renderer.as_mut().ok_or("renderer missing")?;
        // The snapshot is rendered from the runtime mesh library and the engine's material tables.
        // Those tables are not mutated by the draw.
        unsafe {
            *outcome = renderer
                .render_target(target, &snapshot, &*meshes, &*materials, &*textures)
                .map_err(describe)?;
        }
        Ok(true)
    }

    fn push_runtime_camera(&mut self, snapshot: &jarvig_core::RenderSceneSnapshot) -> Result<(), String> {
        let (Some(controller), Some(view)) = (self.editor_camera.as_ref(), self.viewport_view) else {
            return Ok(());
        };
        let frame = controller.reference_frame;
        let (pose, fov, near) = if let Some(id) = self.game.active_camera() {
            let camera = snapshot.game_cameras().iter().find(|camera| camera.entity == id);
            let Some(camera) = camera else {
                return Ok(());
            };
            if camera.orthographic && !self.play_noted_ortho {
                self.play_noted_ortho = true;
                self.append("The startup camera is orthographic. Play keeps infinite reversed-Z perspective.");
            }
            (camera.pose, camera.vertical_fov_radians, camera.near_m)
        } else {
            if !self.play_noted_missing_camera {
                self.play_noted_missing_camera = true;
                self.append("Play has no startup camera. The editor camera was not used. View > Set Selected as Startup Camera names one.");
            }
            let world = self.game.runtime_world().ok_or("runtime world missing")?;
            let front = world.front_camera();
            let pose = snapshot.camera(front.frame).ok_or("runtime view frame missing")?.pose;
            (pose, front.vertical_fov_radians, front.near_m)
        };
        let view_camera = jarvig_core::Camera { frame, vertical_fov_radians: fov, near_m: near };
        if let Some(renderer) = self.renderer.as_mut() {
            renderer
                .update_view(view, RenderViewUpdate { camera: Some(view_camera), layout: None, settings: None, pose: Some(pose) })
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn game_host_exe() -> Option<std::path::PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let sibling = exe.parent()?.join("JARVIGGame.exe");
        sibling.is_file().then_some(sibling)
    }

    fn run_standalone(&mut self) {
        let Some(project) = self.project_file.clone() else {
            self.append("Run Standalone needs an open project.");
            return;
        };
        let Some(host) = Self::game_host_exe() else {
            self.append("JARVIGGame.exe is not next to the editor. Build the jarvig_game package first.");
            return;
        };
        match std::process::Command::new(&host).arg(&project).spawn() {
            Ok(_) => self.append(&format!("Started standalone {} for {}.", host.display(), project.display())),
            Err(error) => self.append(&format!("Standalone game did not start: {error}.")),
        }
    }

    fn build_project(&mut self, and_run: bool) {
        let Some(project) = self.project_file.clone() else {
            self.append("Build Project needs an open project.");
            return;
        };
        let Some(host) = Self::game_host_exe() else {
            self.append("JARVIGGame.exe is not next to the editor. Build the jarvig_game package first.");
            return;
        };
        match std::process::Command::new(&host).arg("--stage").arg(&project).status() {
            Ok(status) if status.success() => {
                let out = project.parent().unwrap_or(std::path::Path::new(".")).join("Build").join("Windows-x64");
                self.append(&format!("Staged a loose development build at {}. No pak and no cook.", out.display()));
                if and_run {
                    let game = out.join("Game.exe");
                    match std::process::Command::new(&game).spawn() {
                        Ok(_) => self.append(&format!("Started {}.", game.display())),
                        Err(error) => self.append(&format!("The staged game did not start: {error}.")),
                    }
                }
            }
            Ok(status) => self.append(&format!("Build Project failed ({status}). The project was not packaged.")),
            Err(error) => self.append(&format!("Build Project did not start: {error}.")),
        }
    }

    fn build_settings(&mut self) {
        let path = self
            .project_file
            .as_ref()
            .and_then(|file| file.parent())
            .map(|root| root.join("Build").join("Windows-x64"))
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "the open project's Build/Windows-x64 directory".into());
        let text = wide(&format!(
            "Development output:\n{path}\n\nLoose files: Game.exe, the project file, and Content.\nNo pak, no cook, and no installer."
        ));
        let title = wide("Build Settings");
        unsafe { MessageBoxW(self.frame, text.as_ptr(), title.as_ptr(), MB_OK); }
    }

    fn begin_play(&mut self) {
        if self.play == PlayPhase::Playing || self.play == PlayPhase::StartingPlay {
            return;
        }
        if self.play == PlayPhase::Paused {
            if self.game.resume().is_err() {
                self.append("Resume failed. The runtime world was not recreated.");
                return;
            }
            self.play = PlayPhase::Playing;
            self.append("Play resumed. The same runtime world continues.");
            self.refresh_status();
            self.invalidate_play_chrome();
            return;
        }
        self.cancel_gizmo();
        self.end_capture(true, true);
        let Some(controller) = self.editor_camera.as_ref() else {
            self.append("Play needs the editor camera. It was not started.");
            return;
        };
        let pose = controller.pose();
        let fly_yaw = controller.yaw;
        let fly_pitch = controller.pitch;
        let fly_position = controller.position;
        let uuid = self.level_uuid.unwrap_or_else(jarvig_core::EntityId::new);
        let name = if self.level_name.is_empty() { "Level" } else { &self.level_name };
        let revision = self.engine.world().revision();
        let saved = self.saved_revision;
        let document = match jarvig_core::LevelDocument::capture(self.engine.world(), uuid, name) {
            Ok(document) => document,
            Err(error) => {
                self.append(&format!("Play did not start. The authored level could not be instantiated: {error}."));
                return;
            }
        };
        self.play = PlayPhase::StartingPlay;
        if let Err(error) = self.game.load(document) {
            self.play = PlayPhase::Editing;
            self.append(&format!("Play did not start. {error}."));
            return;
        }
        self.game.set_mesh_assets(self.engine.mesh_asset_library().clone());
        if let Err(error) = self.game.start() {
            self.play = PlayPhase::Editing;
            self.append(&format!("Play did not start. {error}."));
            return;
        }
        if let Ok(world) = self.game.runtime_world_mut() {
            world.alias_draw_keys_from(self.engine.world());
        }
        let settings = self.play_settings();
        if let Ok(world) = self.game.runtime_world_mut() {
            match jarvig_core::PlayControl::attach(world, &settings) {
                Ok(mut control) => {
                    let mut seeded = false;
                    if settings.pawn == jarvig_core::PawnSelection::DefaultFreeFly && settings.startup_camera == jarvig_core::StartupCameraPolicy::Pawn {
                        if let Some(pawn) = control.pawn() {
                            if let Ok(local) = world.local_translation_for_world_point(pawn, fly_position) {
                                seeded = control.place_free_fly_view(world, local, fly_yaw, fly_pitch).is_ok();
                            }
                        }
                    }
                    let camera = control.pawn_camera();
                    let possessed = control.pawn().is_some();
                    self.play_control = control;
                    if let Some(id) = camera {
                        let _ = self.game.possess_camera(id);
                    }
                    if possessed {
                        self.begin_play_capture();
                        if seeded {
                            self.append("Play started the level at this view. WASD moves. The mouse looks. E and Space rise. Q and Ctrl descend. Shift sprints. Escape releases the mouse. Click captures it again. Stop returns to the authored level.");
                        } else {
                            self.append("Play possessed the project pawn. The mouse is captured. Escape releases it. Click captures it again. The editor camera was not used.");
                        }
                    }
                }
                Err(error) => self.append(&format!("Play did not possess a pawn: {error}.")),
            }
        }
        if self.engine.world().revision() != revision || self.saved_revision != saved {
            self.append("Play changed the authored world. That is a bug.");
        }
        self.play_camera = Some(pose);
        self.play_selection = self.selection.items().to_vec();
        self.pilot_entity = None;
        self.play_noted_missing_camera = false;
        self.play_noted_ortho = false;
        self.play = PlayPhase::Playing;
        self.selection_view_ready = false;
        self.tree_keys.clear();
        self.sync_outliner();
        let camera = self.game.active_camera().map(|id| id.to_string()).unwrap_or_else(|| "none".into());
        self.append(&format!("Play instantiated a runtime world. Startup camera: {camera}. The authored level was not changed."));
        self.refresh_status();
        self.invalidate_play_chrome();
    }

    fn pause_play(&mut self) {
        if self.play != PlayPhase::Playing {
            self.append("Pause does nothing. Play is not running.");
            return;
        }
        if self.game.pause().is_err() {
            self.append("Pause failed.");
            return;
        }
        self.play = PlayPhase::Paused;
        self.append("Play paused. The runtime world is still on screen. Simulation time is frozen.");
        self.refresh_status();
        self.sync_outliner();
        self.invalidate_play_chrome();
    }

    fn stop_play(&mut self) {
        if !self.session_active() && self.play != PlayPhase::StartingPlay {
            return;
        }
        self.play = PlayPhase::StoppingPlay;
        self.end_play_capture();
        self.play_control = jarvig_core::PlayControl::inactive();
        if matches!(self.game.phase(), jarvig_core::ApplicationPhase::Running | jarvig_core::ApplicationPhase::Paused) {
            let _ = self.game.stop();
        }
        if let Some(pose) = self.play_camera.take() {
            if let Some(controller) = self.editor_camera.as_mut() {
                controller.reset_to(pose);
            }
        }
        self.pilot_entity = None;
        let selection = std::mem::take(&mut self.play_selection);
        let _ = self.selection.replace_many(&selection);
        self.play = PlayPhase::Editing;
        let _ = self.push_editor_camera();
        self.selection_view_ready = false;
        self.tree_keys.clear();
        self.sync_outliner();
        self.append("Play stopped. The runtime world was dropped. The editor camera and the authored level are unchanged.");
        self.refresh_status();
        self.invalidate_play_chrome();
    }

    fn invalidate_play_chrome(&self) {
        unsafe {
            InvalidateRect(self.dock_host, std::ptr::null(), 0);
            InvalidateRect(self.toolbar, std::ptr::null(), 0);
            InvalidateRect(self.panel_hwnd(OUTLINER), std::ptr::null(), 1);
        }
    }

    fn set_startup_camera(&mut self) {
        if self.session_active() {
            self.append("Startup camera is authored. Stop play before changing it.");
            return;
        }
        let Some(id) = self.selection.primary_entity() else {
            self.append("Select a Camera actor first.");
            return;
        };
        match self.engine.world_mut().set_startup_camera(Some(id)) {
            Ok(_) => {
                self.sync_outliner();
                self.append(&format!("Startup camera is {id}. Play will use that Camera. The editor camera was not changed."));
            }
            Err(_) => self.append("That actor has no Camera component. The startup camera was not changed."),
        }
    }

    fn tick_editor_camera(&mut self, delta: f64) -> Result<(), String> {
        if self.session_active() {
            self.refresh_status();
            return Ok(());
        }
        if self.pilot_entity.is_some() {
            self.push_pilot_camera()?;
            self.refresh_status();
            return Ok(());
        }
        if self.capture == Some(CaptureKind::Gizmo) {
            self.push_editor_camera()?;
            self.refresh_status();
            return Ok(());
        }
        if self.navigation_open() {
            self.nav_keys.publish(&mut self.nav);
            if let Some(controller) = self.editor_camera.as_mut() {
                controller.tick(&mut self.nav, delta);
            }
        } else {
            self.nav_keys = NavKeys::default();
            self.nav.clear_gestures();
            if self.capture.is_some() || self.cursor_hidden {
                self.end_capture(true, true);
            }
        }
        self.push_editor_camera()?;
        self.refresh_status();
        Ok(())
    }

    fn create_camera_actor(&mut self) {
        if self.session_active() {
            self.append("Create Camera Actor is an authoring command. Stop play first.");
            return;
        }
        let handle = self.engine.world_mut().create_entity("Camera Actor");
        let Ok(id) = self.engine.world().resolve(handle) else {
            self.append("Camera Actor was created, but its id could not be read.");
            self.sync_outliner();
            return;
        };
        if let Err(error) = self.engine.execute_authoring(AuthoringCommand::AddComponent { target: id, type_id: TYPE_SPATIAL_FRAME }) {
            self.append(&format!("Camera Actor transform failed: {error}."));
            self.sync_outliner();
            return;
        }
        if let Err(error) = self.engine.execute_authoring(AuthoringCommand::AddComponent { target: id, type_id: TYPE_CAMERA }) {
            self.append(&format!("Camera Actor component failed: {error}."));
            self.sync_outliner();
            return;
        }
        // Local to the scene frame. Identity looks down -Z, toward the lab around the origin.
        let _ = self.engine.world_mut().set_entity_local_translation(id, Vec3::new(0.0, 1.6, 8.0));
        match selection::SelectionItem::entity(id) {
            Ok(item) => {
                if let Err(error) = self.selection.replace(item) {
                    self.append(&format!("Camera Actor was created but could not be selected: {error}."));
                }
            }
            Err(error) => self.append(&format!("Camera Actor was created but could not be selected: {error}.")),
        }
        self.sync_outliner();
        self.append("Created Camera Actor with Transform and Camera at local (0, 1.6, 8). View > Pilot Selected Camera previews it. Choose that command again to return to the editor camera.");
    }

    fn toggle_pilot_camera(&mut self) {
        if self.pilot_entity.take().is_some() {
            self.append("Perspective editor camera restored.");
            let _ = self.push_editor_camera();
            self.sync_view_menu();
            self.refresh_status();
            return;
        }
        let Some(id) = self.selection.primary_entity() else {
            self.append("Pilot needs a selected Camera actor. The editor camera was not changed.");
            return;
        };
        if self.engine.world().authored_camera(id).is_none() {
            self.append("The selection has no Camera component. The editor camera was not changed.");
            return;
        }
        self.pilot_entity = Some(id);
        if self.engine.world().authored_camera(id).is_some_and(|camera| camera.orthographic) {
            self.append("This camera is orthographic. The editor preview uses its transform and keeps infinite reversed-Z perspective.");
        }
        self.append("Piloting the selected Camera. View > Pilot Selected Camera returns to the editor camera.");
        let _ = self.push_pilot_camera();
        self.sync_view_menu();
        self.refresh_status();
    }

    fn push_pilot_camera(&mut self) -> Result<(), String> {
        let Some(id) = self.pilot_entity else {
            return self.push_editor_camera();
        };
        let Some(camera) = self.engine.world().authored_camera(id) else {
            self.pilot_entity = None;
            self.append("Pilot stopped. That actor no longer has a Camera.");
            return self.push_editor_camera();
        };
        let pose = self.engine.world().entity_world_pose(id).map_err(|error| error.to_string())?;
        let (Some(controller), Some(view)) = (self.editor_camera.as_ref(), self.viewport_view) else {
            return Ok(());
        };
        let view_camera = jarvig_core::Camera {
            frame: controller.reference_frame,
            vertical_fov_radians: camera.vertical_fov_deg.to_radians(),
            near_m: camera.near_m,
        };
        if let Some(renderer) = self.renderer.as_mut() {
            renderer
                .update_view(view, RenderViewUpdate { camera: Some(view_camera), layout: None, settings: None, pose: Some(pose) })
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn push_editor_camera(&mut self) -> Result<(), String> {
        let (Some(controller), Some(view)) = (self.editor_camera.as_ref(), self.viewport_view) else {
            return Ok(());
        };
        let pose = controller.pose();
        let camera = jarvig_core::Camera {
            frame: controller.reference_frame,
            vertical_fov_radians: controller.vertical_fov_radians,
            near_m: controller.near_m,
        };
        if let Some(renderer) = self.renderer.as_mut() {
            renderer
                .update_view(view, RenderViewUpdate { camera: Some(camera), layout: None, settings: None, pose: Some(pose) })
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn reset_editor_camera(&mut self) -> Result<(), String> {
        let home = self.camera_home.ok_or("camera home missing")?;
        if let Some(controller) = self.editor_camera.as_mut() {
            controller.reset_to(home);
        }
        self.push_editor_camera()?;
        let pose = self.editor_camera.as_ref().ok_or("camera missing")?.pose();
        if pose.translation != home.translation {
            return Err("camera reset did not restore the bootstrap pose".into());
        }
        if !self.camera_report.contains("camera_reset=PASS") {
            self.camera_report.push_str("camera_reset=PASS\n");
        }
        Ok(())
    }

    fn focus_character_body(&mut self) {
        let Some((origin, radius)) = self.character_bounds(None) else { return };
        if let Some(camera) = self.editor_camera.as_mut() {
            let _ = camera.focus_origin(origin, radius);
        }
        let _ = self.push_editor_camera();
    }

    fn character_bounds(&self, root_name: Option<&str>) -> Option<(jarvig_core::Vec3, f64)> {
        let outline = self.engine.world().entity_outline();
        let mut min = [f64::MAX; 3];
        let mut max = [f64::MIN; 3];
        let mut any = false;
        for row in &outline {
            if let Some(name) = root_name {
                if !row_under_root(&outline, row.uuid, name) {
                    continue;
                }
            }
            let Ok(focus) = self.engine.world().entity_focus(row.uuid) else { continue };
            if !focus.renderable {
                continue;
            }
            let radius = focus.radius_m;
            min[0] = min[0].min(focus.origin.x - radius);
            min[1] = min[1].min(focus.origin.y - radius);
            min[2] = min[2].min(focus.origin.z - radius);
            max[0] = max[0].max(focus.origin.x + radius);
            max[1] = max[1].max(focus.origin.y + radius);
            max[2] = max[2].max(focus.origin.z + radius);
            any = true;
        }
        if !any {
            return None;
        }
        let center = jarvig_core::Vec3::new((min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, (min[2] + max[2]) * 0.5);
        let extent_x = max[0] - min[0];
        let extent_y = max[1] - min[1];
        let extent_z = max[2] - min[2];
        let radius = (extent_x * extent_x + extent_y * extent_y + extent_z * extent_z).sqrt() * 0.5;
        Some((center, radius.max(0.5)))
    }

    fn step_bind_pose_shots(&mut self) {
        if self.bind_pose_dir.is_none() || self.frames < 12 || self.bind_pose_index >= 6 {
            return;
        }
        if self.bind_pose_capture_next {
            let (body, view, _) = bind_shot(self.bind_pose_index);
            let path = self.bind_pose_dir.as_ref().unwrap().join(format!("{}-{view}.png", body.replace(' ', "-")));
            match capture_window_image(self.frame).and_then(|image| write_png(&path.display().to_string(), &image)) {
                Ok(()) => println!("BIND_POSE_SHOT {}", path.display()),
                Err(error) => {
                    println!("BIND_POSE_SHOTS_FAIL {error}");
                    unsafe { PostQuitMessage(1); }
                    return;
                }
            }
            self.bind_pose_index = self.bind_pose_index.saturating_add(1);
            if self.bind_pose_index >= 6 {
                println!("BIND_POSE_SHOTS_OK");
                unsafe { PostQuitMessage(0); }
                return;
            }
        }
        if let Err(error) = self.aim_bind_pose_shot() {
            println!("BIND_POSE_SHOTS_FAIL {error}");
            unsafe { PostQuitMessage(1); }
            return;
        }
        self.bind_pose_capture_next = true;
    }

    fn aim_bind_pose_shot(&mut self) -> Result<(), String> {
        self.show_joint_debug = true;
        self.show_all_joints = true;
        self.show_joint_limits = false;
        let (body, _, yaw) = bind_shot(self.bind_pose_index);
        let (center, radius) = self.character_bounds(Some(body)).ok_or_else(|| format!("{body} has no mesh bounds"))?;
        let camera = self.editor_camera.as_mut().ok_or("camera missing")?;
        camera.yaw = yaw;
        camera.pitch = -0.05;
        if !camera.focus_origin(center, radius) {
            return Err(format!("{body} was not framed"));
        }
        self.push_editor_camera()
    }

    fn focus_selected(&mut self) -> bool {
        if self.capture == Some(CaptureKind::Look) {
            return false;
        }
        let Some(id) = self.selection.primary_entity() else {
            self.append("Focus did nothing. Nothing is selected.");
            return false;
        };
        match self.engine.world().entity_focus(id) {
            Ok(target) => self
                .editor_camera
                .as_mut()
                .map(|controller| controller.focus_origin(target.origin, target.radius_m))
                .unwrap_or(false),
            Err(FocusError::NoSpatial) => {
                self.append("Focus did nothing. The entity has no spatial frame.");
                false
            }
            Err(FocusError::Missing) => {
                self.append("Focus did nothing. The entity is missing.");
                false
            }
        }
    }

    fn viewport_ray(&self, x: f64, y: f64) -> Option<jarvig_core::PickRay> {
        let camera = self.editor_camera.as_ref()?;
        let renderer = self.renderer.as_ref()?;
        let (width, height) = renderer.configured_size();
        if width == 0 || height == 0 {
            return None;
        }
        perspective_ray(camera.pose(), camera.vertical_fov_radians, width, height, x, y)
    }

    fn gizmo_target(&self) -> Option<(EntityUuid, ResolvedPose)> {
        if self.session_active() {
            return None;
        }
        if !matches!(self.object_tool, chrome::ToolbarCommand::Translate | chrome::ToolbarCommand::Rotate) {
            return None;
        }
        let entity = self.selection.primary_entity()?;
        let pose = self.engine.world().entity_world_pose(entity).ok()?;
        Some((entity, pose))
    }

    fn gizmo_length(&self, origin: Vec3) -> Option<f64> {
        let camera = self.editor_camera.as_ref()?;
        let height = self.renderer.as_ref()?.configured_size().1;
        let distance = vec_len(vec_sub(origin, camera.position));
        Some(gizmo::visual_length(distance, camera.vertical_fov_radians, height))
    }

    fn gizmo_handle_at(&self, x: f64, y: f64) -> Option<gizmo::GizmoHandle> {
        let (entity, pose) = self.gizmo_target()?;
        let ray = self.viewport_ray(x, y)?;
        let length = self.gizmo_length(pose.translation)?;
        let _ = entity;
        gizmo::hit_gizmo(
            pose.translation,
            ray.origin,
            ray.direction,
            length,
            self.transform_space,
            pose.rotation,
            self.object_tool == chrome::ToolbarCommand::Rotate,
        )
    }

    fn update_gizmo_hover(&mut self, x: f64, y: f64) {
        let hover = if self.gizmo_drag.is_some() { self.gizmo_drag.as_ref().map(|drag| drag.handle) } else { self.gizmo_handle_at(x, y) };
        if hover != self.gizmo_hover {
            self.gizmo_hover = hover;
        }
    }

    fn viewport_press(&mut self, hwnd: HWND, x: f64, y: f64, ctrl: bool) {
        // An axis or ring under the cursor starts a drag before a marker or a mesh can change the selection.
        if let Some(handle) = self.gizmo_handle_at(x, y) {
            self.begin_gizmo_drag(hwnd, handle, x, y);
            return;
        }
        // Markers are pick targets only while the Character Editor is choosing a socket.
        if self.character_workspace {
            if let Some(entity) = self.joint_pivot_at(x, y) {
                let item = selection::SelectionItem::Entity(entity);
                if ctrl {
                    let _ = self.selection.toggle_from(item, selection::SelectionSource::Viewport);
                } else {
                    let _ = self.selection.replace_from(item, selection::SelectionSource::Viewport);
                }
                self.sync_selection_view();
                return;
            }
        }
        self.pick_requests = self.pick_requests.saturating_add(1);
        self.pick_broadphase_us = 0;
        self.pick_mesh_us = 0;
        let ray = self.viewport_ray(x, y);
        let hit = ray.and_then(|ray| {
            let snapshot = self.engine.world().extract(RenderFrameId(30)).ok()?;
            let hidden = self.land_hidden_entities();
            let (hit, timings) = pick_snapshot_skipping(ray, &snapshot, self.engine.world().meshes(), &hidden);
            self.pick_broadphase_us = timings.broadphase_us;
            self.pick_mesh_us = timings.mesh_us;
            hit
        });
        if self.meshlet_highlight {
            if let (Some(ray), Some(hit)) = (ray, hit) {
                self.highlight_meshlet_under_cursor(ray, hit.entity);
            }
        }
        let started = Instant::now();
        match hit {
            Some(hit) => {
                self.pick_hits = self.pick_hits.saturating_add(1);
                let entity = self.engine.world().terrain_owner(hit.entity).unwrap_or(hit.entity);
                let item = selection::SelectionItem::Entity(entity);
                if ctrl {
                    let _ = self.selection.toggle_from(item, selection::SelectionSource::Viewport);
                } else {
                    let _ = self.selection.replace_from(item, selection::SelectionSource::Viewport);
                }
                self.selection_change_us = started.elapsed().as_micros();
                self.sync_selection_view();
            }
            None => {
                self.pick_misses = self.pick_misses.saturating_add(1);
                if !ctrl {
                    self.selection.clear_from(selection::SelectionSource::Viewport);
                    self.selection_change_us = started.elapsed().as_micros();
                    self.sync_selection_view();
                }
            }
        }
    }

    fn begin_gizmo_drag(&mut self, hwnd: HWND, handle: gizmo::GizmoHandle, x: f64, y: f64) {
        let Some(entity) = self.selection.primary_entity() else { return };
        let Ok(pose) = self.engine.world().entity_world_pose(entity) else { return };
        let Ok(inspection) = self.engine.world().inspect_entity(entity) else { return };
        let Some(local) = inspection.sections.get(1).and_then(|section| section.fields.first()).and_then(|field| match field.value {
            PropertyValue::Vec3(value) => Some(value),
            _ => None,
        }) else { return };
        let Some(rotation) = inspection.sections.get(1).and_then(|section| section.fields.get(1)).and_then(|field| match field.value {
            PropertyValue::Quat(value) => Some(value),
            _ => None,
        }) else { return };
        let Some(ray) = self.viewport_ray(x, y) else { return };
        let axis = gizmo::axis_in_space(handle, self.transform_space, pose.rotation);
        let normal = if handle.is_rotation() {
            gizmo::rotation_constraint_normal(axis, ray.direction)
        } else {
            gizmo::axis_drag_plane(axis, ray.direction)
        };
        let Some(hit) = gizmo::ray_plane(ray.origin, ray.direction, pose.translation, normal) else { return };
        self.gizmo_drag = Some(gizmo::GizmoDrag {
            entity,
            handle,
            origin: pose.translation,
            axis,
            original_local: local,
            original_rotation: rotation,
            original_world_rotation: pose.rotation,
            start_hit: hit,
        });
        self.gizmo_hover = Some(handle);
        self.begin_capture(hwnd, CaptureKind::Gizmo);
    }

    fn update_gizmo_drag(&mut self, x: f64, y: f64) {
        let Some(drag) = self.gizmo_drag else { return };
        let Some(ray) = self.viewport_ray(x, y) else { return };
        let normal = if drag.handle.is_rotation() {
            gizmo::rotation_constraint_normal(drag.axis, ray.direction)
        } else {
            gizmo::axis_drag_plane(drag.axis, ray.direction)
        };
        let Some(hit) = gizmo::ray_plane(ray.origin, ray.direction, drag.origin, normal) else { return };
        self.apply_drag_hit(hit);
    }

    fn apply_drag_hit(&mut self, hit: Vec3) {
        let Some(drag) = self.gizmo_drag else { return };
        let entity = drag.entity;
        if drag.handle.is_rotation() {
            let Some(angle) = gizmo::rotation_angle(drag.origin, drag.axis, drag.start_hit, hit) else { return };
            let local = if self.transform_space == gizmo::TransformSpace::Local {
                let axis = drag.handle.axis();
                gizmo::local_rotation_after(drag.original_rotation, axis, angle)
            } else {
                gizmo::world_rotation_after(drag.original_world_rotation, drag.axis, angle)
                    .and_then(|world| self.engine.world().local_rotation_for_world_rotation(entity, world).ok())
            };
            let Some(local) = local else { return };
            let _ = self.engine.execute_authoring(AuthoringCommand::SetProperty {
                target: entity,
                type_id: jarvig_core::TYPE_SPATIAL_FRAME,
                field: jarvig_core::FIELD_LOCAL_ROTATION,
                value: PropertyValue::Quat(local),
            });
        } else {
            let world_delta = gizmo::translation_delta(&drag, hit);
            self.write_translation_delta(entity, drag.original_local, world_delta);
        }
        self.gizmo_updates = self.gizmo_updates.saturating_add(1);
    }

    fn write_translation_delta(&mut self, entity: EntityUuid, baseline: Vec3, world_delta: Vec3) {
        let Ok(local) = self.engine.world().local_translation_plus_world_delta(entity, baseline, world_delta) else { return };
        let _ = self.engine.execute_authoring(AuthoringCommand::SetProperty {
            target: entity,
            type_id: jarvig_core::TYPE_SPATIAL_FRAME,
            field: jarvig_core::FIELD_LOCAL_TRANSLATION,
            value: PropertyValue::Vec3(local),
        });
    }

    fn commit_gizmo(&mut self) {
        if self.gizmo_drag.take().is_some() {
            self.gizmo_commits = self.gizmo_commits.saturating_add(1);
            self.sync_outliner();
        }
        if self.capture == Some(CaptureKind::Gizmo) {
            self.end_capture(true, false);
        }
    }

    fn cancel_gizmo(&mut self) {
        if self.gizmo_drag.is_some() {
            self.restore_gizmo_drag();
        }
        if self.capture == Some(CaptureKind::Gizmo) {
            self.end_capture(true, false);
        }
    }

    fn restore_gizmo_drag(&mut self) {
        let Some(drag) = self.gizmo_drag.take() else { return };
        let _ = self.engine.execute_authoring(AuthoringCommand::SetProperty {
            target: drag.entity,
            type_id: jarvig_core::TYPE_SPATIAL_FRAME,
            field: jarvig_core::FIELD_LOCAL_TRANSLATION,
            value: PropertyValue::Vec3(drag.original_local),
        });
        let _ = self.engine.execute_authoring(AuthoringCommand::SetProperty {
            target: drag.entity,
            type_id: jarvig_core::TYPE_SPATIAL_FRAME,
            field: jarvig_core::FIELD_LOCAL_ROTATION,
            value: PropertyValue::Quat(drag.original_rotation),
        });
        self.gizmo_cancels = self.gizmo_cancels.saturating_add(1);
        self.sync_outliner();
    }

    fn publish_gizmo(&mut self) {
        let vertices = self.gizmo_vertices().unwrap_or_default();
        let view = self.viewport_view;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_editor_overlay(match view {
                Some(view) if !vertices.is_empty() => Some(EditorOverlay { view, vertices }),
                _ => None,
            });
        }
    }

    fn toggle_meshlet_freeze(&mut self) {
        self.meshlet_freeze = !self.meshlet_freeze;
        self.sync_view_menu();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_meshlet_freeze(self.meshlet_freeze);
        }
        if self.meshlet_freeze {
            self.append("Meshlet visibility is frozen. Move the camera. If the roof stays, the geometry is present and the live test was changing the set. Turn this off to test again.");
        } else {
            self.append("Meshlet visibility follows the camera again.");
        }
        self.refresh_status();
    }

    fn toggle_cluster_hierarchy(&mut self) {
        self.cluster_hierarchy = !self.cluster_hierarchy;
        self.sync_view_menu();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_cluster_hierarchy(self.cluster_hierarchy);
        }
        if self.cluster_hierarchy {
            self.append("Cluster hierarchy is on. Leaf meshlets stay on screen until the parent-mesh job finishes. The status line shows that job.");
            self.sync_parent_geometry();
        } else {
            if let Some(job) = self.parent_job.take() {
                self.jobs.cancel(job.id);
                let _ = self.jobs.take_result::<jarvig_core::ParentGeometry>(job.id);
            }
            self.job_line.clear();
            self.append("Cluster hierarchy is off. Draw From Meshlets uses the accepted leaf set.");
        }
        self.refresh_status();
    }

    fn toggle_microgeometry(&mut self) {
        self.micro_enabled = !self.micro_enabled;
        if !self.micro_enabled {
            self.micro_color = false;
        }
        self.sync_view_menu();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_microgeometry(self.micro_enabled, self.micro_seed, self.micro_color);
        }
        self.append(if self.micro_enabled {
            "Procedural microgeometry is on. Above 1 px it adds Einstein patches on the selected surface. The ordinary mesh and the hierarchy sidecar stay untouched."
        } else {
            "Procedural microgeometry is off. The townshop is the ordinary surface again."
        });
        self.refresh_status();
    }

    fn toggle_micro_color(&mut self) {
        self.micro_color = !self.micro_color;
        if self.micro_color {
            self.micro_enabled = true;
            self.einstein_debug = false;
        }
        self.sync_view_menu();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_einstein_debug(self.einstein_debug, self.einstein_seed);
            renderer.set_microgeometry(self.micro_enabled, self.micro_seed, self.micro_color);
        }
        self.append(if self.micro_color {
            "Microtriangle colors are on. Generated triangles draw orange. The shaded material is still the path when this is off."
        } else {
            "Microtriangle colors are off. Generated triangles use the surface material."
        });
        self.refresh_status();
    }

    fn micro_status(&self) -> String {
        if !self.micro_enabled {
            return String::new();
        }
        let stats = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats());
        let base = stats.map(|stats| stats.hierarchy_lod_triangles).unwrap_or(0);
        let triangles = stats.map(|stats| stats.micro_triangles).unwrap_or(0);
        let patches = stats.map(|stats| stats.micro_patches).unwrap_or(0);
        let samples = stats.map(|stats| stats.micro_samples).unwrap_or(0);
        let vertices = stats.map(|stats| stats.micro_vertices).unwrap_or(0);
        let generation = stats.map(|stats| stats.micro_generation_us).unwrap_or(0);
        let upload = stats.map(|stats| stats.micro_upload_us).unwrap_or(0);
        let fallbacks = stats.map(|stats| stats.micro_fallbacks).unwrap_or(0);
        let projected = self.shop_detail_px().unwrap_or(0.0);
        format!(
            "base tris {} | micro tris +{} | patches {} | samples {} | verts {} | gen {} us | upload {} us | queue {} us | publish {} us | reuse {} | cancelled {} | stale {} | {} | fallback {} | projected {:.2} px | seed {} | ",
            grouped(base),
            grouped(triangles),
            patches,
            samples,
            vertices,
            generation,
            upload,
            stats.map(|stats| stats.micro_queue_wait_us).unwrap_or(0),
            stats.map(|stats| stats.micro_publish_us).unwrap_or(0),
            stats.map(|stats| stats.micro_reused).unwrap_or(0),
            stats.map(|stats| stats.micro_cancelled).unwrap_or(0),
            stats.map(|stats| stats.micro_stale_discarded).unwrap_or(0),
            self.renderer.as_ref().map(|renderer| renderer.micro_stage()).unwrap_or("Ready"),
            fallbacks,
            projected,
            self.micro_seed
        )
    }

    fn micro_distance(&self, name: &str) -> f64 {
        self.rfc_distances.iter().find(|(label, _)| label == name).map(|(_, distance)| *distance).unwrap_or(2.4)
    }

    fn micro_publish_settled(&self) -> bool {
        let job_busy = self.micro_job.as_ref().is_some_and(|job| !job.finished);
        let ready = self.renderer.as_ref().map(|renderer| renderer.micro_publish_settled()).unwrap_or(true);
        !job_busy && ready
    }

    fn await_micro_publish(&mut self) -> bool {
        if self.micro_publish_settled() {
            return true;
        }
        if self.lod_hold > 1200 {
            self.transition_reasons.push("\"microgeometry publish did not settle\"".into());
            self.finish_transition();
        }
        false
    }

    fn micro_operation_stage(&self) -> String {
        if let Some(job) = &self.micro_job {
            if !job.finished {
                if let Some(snap) = self.jobs.snapshots().iter().find(|snap| snap.id == job.id) {
                    return snap.stage.clone();
                }
                return "Queued".into();
            }
        }
        self.renderer.as_ref().map(|renderer| renderer.micro_stage().to_string()).unwrap_or_else(|| "Ready".into())
    }

    fn step_micro_phases(&mut self) {
        if self.lod_phase != 50 && !self.micro_publish_settled() {
            if self.lod_hold > 1200 {
                println!("RFC0002_MICRO_FAIL publish did not settle");
                self.finish_lod_capture();
            }
            return;
        }
        let wait = if matches!(self.lod_phase, 55 | 56 | 57 | 58 | 60) { 10 } else { 6 };
        if self.lod_hold < wait {
            return;
        }
        match self.lod_phase {
            50 => {
                self.micro_seal = self.townshop_seal();
                self.capture_micro_shot("off-close");
                self.micro_enabled = true;
                self.micro_color = false;
                self.push_micro_state();
                self.lod_phase = 51;
            }
            51 => {
                self.capture_micro_shot("on-close");
                self.lod_phase = 52;
            }
            52 => {
                self.capture_micro_shot("on-close-again");
                if let Some(camera) = self.editor_camera.as_mut() {
                    camera.yaw += 0.45;
                    camera.reorbit();
                }
                self.lod_phase = 53;
            }
            53 => {
                self.capture_micro_shot("on-orbit");
                self.place_shop_camera(self.micro_distance("partial"));
                self.lod_phase = 55;
            }
            55 => {
                self.capture_micro_shot("partial");
                self.place_shop_camera(self.micro_distance("medium"));
                self.lod_phase = 56;
            }
            56 => {
                self.capture_micro_shot("medium");
                self.place_shop_camera(self.micro_distance("far"));
                self.lod_phase = 57;
            }
            57 => {
                self.capture_micro_shot("far");
                self.place_shop_camera(self.micro_distance("near"));
                self.lod_phase = 58;
            }
            58 => {
                self.capture_micro_shot("shaded-near");
                self.micro_color = true;
                self.push_micro_state();
                self.lod_phase = 59;
            }
            59 => {
                self.capture_micro_shot("color-near");
                self.micro_color = false;
                self.micro_enabled = false;
                self.push_micro_state();
                self.place_shop_camera(self.micro_distance("close"));
                self.lod_phase = 60;
            }
            60 => {
                self.capture_micro_shot("disabled-close");
                self.finish_rfc0002_micro();
                return;
            }
            _ => {
                self.finish_rfc0002_micro();
                return;
            }
        }
        self.lod_hold = 0;
    }

    fn push_micro_state(&mut self) {
        self.sync_view_menu();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_einstein_debug(false, self.einstein_seed);
            renderer.set_micro_surface(self.micro_surface);
            renderer.set_microgeometry(self.micro_enabled, self.micro_seed, self.micro_color);
        }
        self.einstein_debug = false;
    }

    fn capture_micro_shot(&mut self, name: &str) {
        self.refresh_status();
        if !self.status.is_null() {
            unsafe { UpdateWindow(self.status); }
        }
        let stats = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats());
        let projected = self.shop_detail_px().unwrap_or(0.0);
        let directory = format!("{}/../target/rfc0002-micro", env!("CARGO_MANIFEST_DIR"));
        let _ = std::fs::create_dir_all(&directory);
        let mut rgba = Vec::new();
        let mut width = 0;
        let mut height = 0;
        let mut region = (0, 0, 1, 1);
        let mut orange = 0.0;
        if let Ok(image) = capture_window_image(self.frame) {
            let _ = write_png(&format!("{directory}/{name}.png"), &image);
            let (origin_x, origin_y, view_w, view_h) = viewport_origin(self.frame, self.panel_hwnd(PERSPECTIVE));
            region = shop_region(self.shop_projection().as_ref(), origin_x, origin_y, view_w, view_h, image.width, image.height);
            orange = orange_fraction(&image.rgba, image.width, region);
            rgba = image.rgba;
            width = image.width;
            height = image.height;
        }
        let shot = MicroShot {
            name: name.into(),
            base_triangles: stats.map(|stats| stats.hierarchy_lod_triangles).unwrap_or(0),
            leaves: stats.map(|stats| stats.hierarchy_leaves).unwrap_or(0),
            leaf_triangles: stats.map(|stats| stats.hierarchy_leaf_triangles).unwrap_or(0),
            patches: stats.map(|stats| stats.micro_patches).unwrap_or(0),
            samples: stats.map(|stats| stats.micro_samples).unwrap_or(0),
            vertices: stats.map(|stats| stats.micro_vertices).unwrap_or(0),
            triangles: stats.map(|stats| stats.micro_triangles).unwrap_or(0),
            generation_us: stats.map(|stats| stats.micro_generation_us).unwrap_or(0),
            upload_us: stats.map(|stats| stats.micro_upload_us).unwrap_or(0),
            fallbacks: stats.map(|stats| stats.micro_fallbacks).unwrap_or(0),
            ordinary: stats.map(|stats| stats.micro_ordinary).unwrap_or(0),
            fingerprint: stats.map(|stats| stats.micro_fingerprint).unwrap_or(0),
            displacement_um: stats.map(|stats| stats.micro_max_displacement_um).unwrap_or(0),
            projected_px: projected,
            orange,
            rgba,
            width,
            height,
            region,
        };
        println!(
            "RFC0002_MICRO_SHOT {name} base={} micro={} patches={} samples={} verts={} ordinary={} fallback={} gen_us={} upload_us={} disp_um={} fingerprint={:x} projected={projected:.2} orange={orange:.3} leaves={} leaf_tris={} image={}x{}",
            shot.base_triangles,
            shot.triangles,
            shot.patches,
            shot.samples,
            shot.vertices,
            shot.ordinary,
            shot.fallbacks,
            shot.generation_us,
            shot.upload_us,
            shot.displacement_um,
            shot.fingerprint,
            shot.leaves,
            shot.leaf_triangles,
            shot.width,
            shot.height
        );
        self.micro_shots.push(shot);
    }

    fn townshop_seal(&self) -> Option<SourceSeal> {
        let entity = self.find_named_entity("townshop")?;
        let snapshot = self.engine.world().extract(jarvig_core::RenderFrameId(77)).ok()?;
        let instance = snapshot.instances().iter().find(|instance| instance.entity == entity)?;
        let mesh = self.engine.world().meshes().get(instance.mesh)?;
        let stream = mesh.streams().first()?;
        let stats = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats());
        let asset = self.engine.world().authored_mesh(entity).and_then(|(_, _, _, _, _, _, mesh, _)| match mesh {
            jarvig_core::MeshAssetRef::Asset { id, .. } => Some(id),
            _ => None,
        });
        let (parents_len, parents_modified) = asset
            .and_then(|asset| self.parent_cache_path(asset))
            .and_then(|path| std::fs::metadata(path).ok())
            .map(|meta| {
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|time| time.as_secs())
                    .unwrap_or(0);
                (meta.len(), modified)
            })
            .unwrap_or((0, 0));
        Some(SourceSeal {
            vertices: mesh.vertex_count(),
            bytes: stream.bytes.len() as u64,
            hash: hash_bytes(&stream.bytes),
            leaf_triangles: stats.map(|stats| stats.hierarchy_leaf_triangles).unwrap_or(0),
            hierarchy_nodes: stats.map(|stats| stats.hierarchy_nodes).unwrap_or(0),
            parents_len,
            parents_modified,
        })
    }

    fn finish_rfc0002_micro(&mut self) {
        let directory = format!("{}/../target/rfc0002-micro", env!("CARGO_MANIFEST_DIR"));
        let _ = std::fs::create_dir_all(&directory);
        let mut reasons = Vec::new();
        let shot = |name: &str| self.micro_shots.iter().find(|shot| shot.name == name);
        let (off, close, again, orbit, partial, medium, far, near, color, disabled) = match (
            shot("off-close"),
            shot("on-close"),
            shot("on-close-again"),
            shot("on-orbit"),
            shot("partial"),
            shot("medium"),
            shot("far"),
            shot("shaded-near"),
            shot("color-near"),
            shot("disabled-close"),
        ) {
            (Some(off), Some(close), Some(again), Some(orbit), Some(partial), Some(medium), Some(far), Some(near), Some(color), Some(disabled)) => {
                (off, close, again, orbit, partial, medium, far, near, color, disabled)
            }
            _ => {
                println!("RFC0002_MICRO_FAIL missing captures");
                self.finish_lod_capture();
                return;
            }
        };
        if off.triangles != 0 || off.vertices != 0 {
            reasons.push(format!("\"feature off drew {} micro triangles\"", off.triangles));
        }
        if close.triangles == 0 || close.triangles > 18432 {
            reasons.push(format!("\"close generated {} micro triangles\"", close.triangles));
        }
        if close.base_triangles != off.base_triangles || close.leaves != off.leaves {
            reasons.push(format!(
                "\"close cut changed {}/{} vs {}/{}\"",
                close.leaves, close.base_triangles, off.leaves, off.base_triangles
            ));
        }
        if partial.triangles == 0 || partial.triangles >= close.triangles {
            reasons.push(format!("\"partial micro {} did not fall below close {}\"", partial.triangles, close.triangles));
        }
        if medium.triangles != 0 {
            reasons.push(format!("\"medium still generated {} micro triangles\"", medium.triangles));
        }
        if far.triangles != 0 {
            reasons.push(format!("\"far still generated {} micro triangles\"", far.triangles));
        }
        if again.fingerprint != close.fingerprint || again.triangles != close.triangles {
            reasons.push("\"repeat camera changed the generated geometry\"".into());
        }
        if orbit.fingerprint != close.fingerprint || orbit.triangles != close.triangles {
            reasons.push("\"orbit changed the generated geometry\"".into());
        }
        for (name, shot) in [("close", close), ("partial", partial), ("near", near)] {
            if shot.displacement_um > 8000 {
                reasons.push(format!("\"{name} displacement {} um exceeds 8000\"", shot.displacement_um));
            }
        }
        let end_seal = self.townshop_seal();
        match (self.micro_seal.as_ref(), end_seal.as_ref()) {
            (Some(start), Some(end)) => {
                if start.vertices != end.vertices || start.bytes != end.bytes || start.hash != end.hash {
                    reasons.push("\"source mesh bytes changed\"".into());
                }
                if start.leaf_triangles != end.leaf_triangles || start.hierarchy_nodes != end.hierarchy_nodes {
                    reasons.push(format!(
                        "\"hierarchy changed leaves {} nodes {} vs {} {}\"",
                        start.leaf_triangles, start.hierarchy_nodes, end.leaf_triangles, end.hierarchy_nodes
                    ));
                }
                if start.parents_len != end.parents_len || start.parents_modified != end.parents_modified {
                    reasons.push("\"jarvigparents changed\"".into());
                }
                if start.leaf_triangles != 6_122_214 {
                    reasons.push(format!("\"leaf triangles are {}\"", start.leaf_triangles));
                }
            }
            _ => reasons.push("\"source seal missing\"".into()),
        }
        if disabled.triangles != 0 {
            reasons.push(format!("\"disable left {} micro triangles\"", disabled.triangles));
        }
        if disabled.base_triangles != off.base_triangles {
            reasons.push(format!("\"disable base {} differs from off {}\"", disabled.base_triangles, off.base_triangles));
        }
        let restore = mean_abs_diff(&off.rgba, &disabled.rgba, off.width, off.region, disabled.region);
        if restore > 8.0 {
            reasons.push(format!("\"disable pixel delta {restore:.2} is above 8\""));
        }
        if color.triangles == 0 || color.fingerprint != near.fingerprint {
            reasons.push("\"color debug changed or dropped the micro triangles\"".into());
        }
        if color.orange <= near.orange || color.orange < near.orange * 2.0 {
            reasons.push(format!("\"color debug orange {:.3} did not rise above shaded {:.3}\"", color.orange, near.orange));
        }
        let passed = reasons.is_empty();
        let verdict = if passed { "PASS" } else { "FAIL" };
        let reasons_json = format!("[{}]", reasons.join(","));
        let seal = self.micro_seal.as_ref();
        let json = format!(
            "{{\"verdict\":\"RFC0002_MICRO_{verdict}\",\"seed\":{},\"amplitude_m\":0.008,\"budget_triangles\":18432,\"off_micro\":{},\"close\":{{\"base\":{},\"micro\":{},\"patches\":{},\"samples\":{},\"verts\":{},\"ordinary\":{},\"fallback\":{},\"generation_us\":{},\"upload_us\":{},\"displacement_um\":{},\"projected_px\":{:.2}}},\"repeat_match\":{},\"orbit_match\":{},\"partial_micro\":{},\"medium_micro\":{},\"far_micro\":{},\"near_micro\":{},\"color_orange\":{:.4},\"shaded_orange\":{:.4},\"disabled_micro\":{},\"restore_delta\":{restore:.3},\"mesh_vertices\":{},\"mesh_hash\":\"{:x}\",\"parents_len\":{},\"reasons\":{reasons_json}}}",
            self.micro_seed,
            off.triangles,
            close.base_triangles,
            close.triangles,
            close.patches,
            close.samples,
            close.vertices,
            close.ordinary,
            close.fallbacks,
            close.generation_us,
            close.upload_us,
            close.displacement_um,
            close.projected_px,
            again.fingerprint == close.fingerprint,
            orbit.fingerprint == close.fingerprint,
            partial.triangles,
            medium.triangles,
            far.triangles,
            near.triangles,
            color.orange,
            near.orange,
            disabled.triangles,
            seal.map(|seal| seal.vertices).unwrap_or(0),
            seal.map(|seal| seal.hash).unwrap_or(0),
            seal.map(|seal| seal.parents_len).unwrap_or(0)
        );
        let _ = std::fs::write(format!("{directory}/report.json"), json);
        println!("RFC0002_MICRO_{verdict} {directory}");
        println!("RFC0002_MICRO_REASONS {reasons_json}");
        self.finish_lod_capture();
    }

    fn step_refine_phases(&mut self) {
        match self.lod_phase {
            90 => {
                if self.lod_hold < 8 {
                    return;
                }
                self.micro_seal = self.townshop_seal();
                self.transition_off = Some(self.capture_relief_shot("shaded-off"));
                self.micro_enabled = true;
                self.micro_surface = false;
                self.micro_color = false;
                self.push_micro_state();
                self.lod_phase = 91;
                self.lod_hold = 0;
            }
            91 => {
                if !self.await_micro_publish() {
                    return;
                }
                if self.lod_hold < 6 {
                    return;
                }
                self.refine_hat = Some(self.capture_relief_shot("einstein-hat"));
                self.micro_surface = true;
                self.push_micro_state();
                self.lod_phase = 92;
                self.lod_hold = 0;
            }
            92 => {
                if !self.await_micro_publish() {
                    return;
                }
                if self.lod_hold < 6 {
                    return;
                }
                self.transition_on = Some(self.capture_relief_shot("refined-shaded"));
                self.micro_color = true;
                self.push_micro_state();
                self.lod_phase = 93;
                self.lod_hold = 0;
            }
            93 => {
                if !self.await_micro_publish() {
                    return;
                }
                if self.lod_hold < 4 {
                    return;
                }
                self.transition_color = Some(self.capture_relief_shot("refined-color"));
                self.judge_refine();
                self.micro_color = false;
                self.micro_surface = true;
                self.push_micro_state();
                let near = self.shop_target().map(|target| target.half_z + 0.55).unwrap_or(1.2).clamp(1.05, 2.2);
                self.place_shop_camera(near * 0.85);
                if let Some(camera) = self.editor_camera.as_mut() {
                    camera.pitch = -0.42;
                    camera.reorbit();
                }
                self.lod_phase = 94;
                self.lod_hold = 0;
            }
            94 => {
                if !self.await_micro_publish() {
                    return;
                }
                if self.lod_hold < 6 {
                    return;
                }
                self.refine_grazing = Some(self.capture_relief_shot("grazing"));
                if let Some(grazing) = self.refine_grazing.as_ref() {
                    if grazing.micro_triangles == 0 || grazing.invalid != 0 || grazing.displacement_um > 8000 {
                        self.transition_reasons.push(format!(
                            "\"grazing view micro {} invalid {} displacement {} um\"",
                            grazing.micro_triangles, grazing.invalid, grazing.displacement_um
                        ));
                    }
                }
                self.transition_index = 0;
                if let Some(distance) = self.transition_distances.first().copied() {
                    self.place_shop_camera(distance);
                }
                self.lod_phase = 80;
                self.lod_hold = 0;
            }
            _ => self.finish_transition(),
        }
    }

    fn judge_refine(&mut self) {
        let directory = format!("{}/../target/rfc0002-refine", env!("CARGO_MANIFEST_DIR"));
        let _ = std::fs::create_dir_all(&directory);
        let gathered = self.transition_off.as_ref().zip(self.refine_hat.as_ref()).zip(self.transition_on.as_ref()).zip(self.transition_color.as_ref()).map(|(((off, hat), refined), color)| {
            (
                off.micro_triangles,
                hat.micro_triangles,
                refined.micro_triangles,
                color.micro_triangles,
                hat.fingerprint,
                refined.fingerprint,
                color.fingerprint,
                refined.invalid,
                color.invalid,
                refined.displacement_um,
                refined.leaf_triangles,
                off.base_triangles,
                relief_metrics(off, refined, color),
                compose_shots(&[off, hat, refined, color]),
            )
        });
        let Some((off_micro, hat_micro, refined_micro, color_micro, hat_fp, refined_fp, color_fp, refined_invalid, color_invalid, displacement_um, leaf_triangles, off_base, metrics, review)) = gathered else {
            self.transition_reasons.push("\"refine comparison captures missing\"".into());
            return;
        };
        println!("RFC0002_REFINE_COMPARE off={off_micro} hat={hat_micro} refined={refined_micro} color={color_micro} hat_fp={hat_fp:x} refined_fp={refined_fp:x} disp_um={displacement_um} invalid={refined_invalid}");
        if off_micro != 0 {
            self.transition_reasons.push(format!("\"feature off drew {off_micro} micro triangles\""));
        }
        if hat_micro == 0 {
            self.transition_reasons.push("\"einstein hat comparison drew no triangles\"".into());
        }
        if refined_micro == 0 || refined_micro > 18432 {
            self.transition_reasons.push(format!("\"refined surface drew {refined_micro} micro triangles\""));
        }
        if refined_fp == hat_fp {
            self.transition_reasons.push("\"refined surface reused the tent fingerprint\"".into());
        }
        if refined_invalid != 0 || color_invalid != 0 {
            self.transition_reasons.push(format!("\"refined surface failed {} validity checks\"", refined_invalid.saturating_add(color_invalid)));
        }
        if displacement_um > 8000 {
            self.transition_reasons.push(format!("\"refined displacement {displacement_um} um exceeds 8000\""));
        }
        if color_fp != refined_fp || color_micro != refined_micro {
            self.transition_reasons.push("\"refined color debug does not match the shaded mesh\"".into());
        }
        if leaf_triangles != 6_122_214 || off_base == 0 {
            self.transition_reasons.push("\"base hierarchy changed during the refine comparison\"".into());
        }
        println!(
            "RFC0002_REFINE_RELIEF mean={:.3} changed={:.4} marks={} overlap={:.3} outside={:.3} hole_ratio={:.4}",
            metrics.mean_diff, metrics.changed_fraction, metrics.marks, metrics.overlap, metrics.outside_diff, metrics.hole_ratio
        );
        if metrics.mean_diff < 0.35 {
            self.transition_reasons.push(format!("\"refined relief mean diff {:.3} is below 0.35\"", metrics.mean_diff));
        }
        if metrics.changed_fraction > 0.35 {
            self.transition_reasons.push(format!("\"refined relief covers {:.4} of the shop\"", metrics.changed_fraction));
        }
        if metrics.marks < 40 || metrics.overlap < 0.20 {
            self.transition_reasons.push(format!("\"refined marks {} overlap {:.3}\"", metrics.marks, metrics.overlap));
        }
        if metrics.solid >= 64 && metrics.hole_ratio > 0.02 {
            self.transition_reasons.push(format!("\"refined silhouette hole ratio {:.4}\"", metrics.hole_ratio));
        }
        if let Some(review) = review {
            let _ = write_png(&format!("{directory}/review.png"), &review);
            println!("RFC0002_REFINE_REVIEW {directory}/review.png");
        }
    }

    fn evidence_directory(&self) -> String {
        let name = if self.rfc0002_refine { "rfc0002-refine" } else { "rfc0002-transition" };
        format!("{}/../target/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    fn transition_path(&mut self) -> Vec<f64> {
        let mut forward = Vec::new();
        let mut pixels = 3.20_f64;
        while pixels >= 0.50 - 1.0e-9 {
            forward.push(self.projected_error_distance(0.02, pixels));
            pixels -= 0.12;
        }
        self.transition_forward = forward.len();
        let mut path = forward.clone();
        for distance in forward.iter().rev().skip(1) {
            path.push(*distance);
        }
        path
    }

    fn step_transition_phases(&mut self) {
        match self.lod_phase {
            70 => {
                if self.lod_hold < 8 {
                    return;
                }
                self.micro_seal = self.townshop_seal();
                self.transition_off = Some(self.capture_relief_shot("shaded-off"));
                self.micro_enabled = true;
                self.micro_color = false;
                self.push_micro_state();
                self.lod_phase = 71;
                self.lod_hold = 0;
            }
            71 => {
                if self.transition_align == 1 {
                    if self.lod_hold < 2 {
                        return;
                    }
                    self.transition_off = Some(self.capture_relief_shot("shaded-off"));
                    self.micro_enabled = true;
                    self.micro_color = false;
                    self.push_micro_state();
                    self.transition_align = 2;
                    self.lod_hold = 0;
                    return;
                }
                if !self.await_micro_publish() {
                    return;
                }
                if self.lod_hold < 6 {
                    return;
                }
                let (width, height) = self.frame_window_size();
                let resized = self.transition_align == 0
                    && self.transition_off.as_ref().is_some_and(|shot| shot.width != width || shot.height != height);
                if resized {
                    println!("RFC0002_TRANSITION_RESIZE {width}x{height}");
                    self.micro_enabled = false;
                    self.micro_color = false;
                    self.push_micro_state();
                    self.transition_align = 1;
                    self.lod_hold = 0;
                    return;
                }
                self.transition_on = Some(self.capture_relief_shot("shaded-on"));
                self.micro_color = true;
                self.push_micro_state();
                self.lod_phase = 72;
                self.lod_hold = 0;
            }
            72 => {
                if !self.await_micro_publish() {
                    return;
                }
                if self.lod_hold < 4 {
                    return;
                }
                self.transition_color = Some(self.capture_relief_shot("color-on"));
                self.judge_relief();
                self.micro_color = false;
                self.push_micro_state();
                self.transition_index = 0;
                if let Some(distance) = self.transition_distances.first().copied() {
                    self.place_shop_camera(distance);
                }
                self.lod_phase = 80;
                self.lod_hold = 0;
            }
            80 => {
                if !self.await_micro_publish() {
                    return;
                }
                self.record_transition_frame();
                self.transition_index = self.transition_index.saturating_add(1);
                if self.transition_index >= self.transition_distances.len() {
                    self.finish_transition();
                    return;
                }
                if let Some(distance) = self.transition_distances.get(self.transition_index).copied() {
                    self.place_shop_camera(distance);
                }
                self.lod_hold = 0;
            }
            _ => self.finish_transition(),
        }
    }

    fn frame_window_size(&self) -> (i32, i32) {
        unsafe {
            let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            if GetWindowRect(self.frame, &mut rect) == 0 {
                return (0, 0);
            }
            (rect.right - rect.left, rect.bottom - rect.top)
        }
    }

    fn capture_relief_shot(&mut self, name: &str) -> ReliefShot {
        self.refresh_status();
        if !self.status.is_null() {
            unsafe { UpdateWindow(self.status); }
        }
        let stats = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats());
        let directory = self.evidence_directory();
        let _ = std::fs::create_dir_all(&directory);
        let mut rgba = Vec::new();
        let mut width = 0;
        let mut height = 0;
        let mut region = (0, 0, 1, 1);
        let mut origin = (0, 0, 1, 1);
        if let Ok(image) = capture_window_image(self.frame) {
            let _ = write_png(&format!("{directory}/{name}.png"), &image);
            origin = viewport_origin(self.frame, self.panel_hwnd(PERSPECTIVE));
            let (origin_x, origin_y, view_w, view_h) = origin;
            region = shop_region(self.shop_projection().as_ref(), origin_x, origin_y, view_w, view_h, image.width, image.height);
            rgba = image.rgba;
            width = image.width;
            height = image.height;
        }
        let shot = ReliefShot {
            rgba,
            width,
            height,
            region,
            origin,
            base_triangles: stats.map(|stats| stats.hierarchy_lod_triangles).unwrap_or(0),
            micro_triangles: stats.map(|stats| stats.micro_triangles).unwrap_or(0),
            vertices: stats.map(|stats| stats.micro_vertices).unwrap_or(0),
            fingerprint: stats.map(|stats| stats.micro_fingerprint).unwrap_or(0),
            invalid: stats.map(|stats| stats.micro_invalid).unwrap_or(0),
            displacement_um: stats.map(|stats| stats.micro_max_displacement_um).unwrap_or(0),
            projected_px: self.shop_detail_px().unwrap_or(0.0),
            generation_us: stats.map(|stats| stats.micro_generation_us).unwrap_or(0),
            upload_us: stats.map(|stats| stats.micro_upload_us).unwrap_or(0),
            reused: stats.map(|stats| stats.micro_reused).unwrap_or(0),
            leaf_triangles: stats.map(|stats| stats.hierarchy_leaf_triangles).unwrap_or(0),
        };
        println!(
            "RFC0002_TRANSITION_SHOT {name} base={} micro={} verts={} invalid={} disp_um={} fingerprint={:x} projected={:.2} gen_us={} upload_us={} reuse={} leaf_tris={} image={}x{}",
            shot.base_triangles,
            shot.micro_triangles,
            shot.vertices,
            shot.invalid,
            shot.displacement_um,
            shot.fingerprint,
            shot.projected_px,
            shot.generation_us,
            shot.upload_us,
            shot.reused,
            shot.leaf_triangles,
            shot.width,
            shot.height
        );
        shot
    }

    fn judge_relief(&mut self) {
        let directory = self.evidence_directory();
        let _ = std::fs::create_dir_all(&directory);
        let compared = self.transition_off.as_ref().zip(self.transition_on.as_ref()).zip(self.transition_color.as_ref()).map(|((off, on), color)| {
            (
                off.micro_triangles,
                on.micro_triangles,
                on.invalid,
                color.invalid,
                on.displacement_um,
                on.base_triangles,
                on.leaf_triangles,
                on.fingerprint,
                color.fingerprint,
                color.micro_triangles,
                relief_metrics(off, on, color),
                compose_review(off, on, color),
            )
        });
        let Some((off_micro, on_micro, on_invalid, color_invalid, displacement_um, base_triangles, leaf_triangles, on_fingerprint, color_fingerprint, color_micro, metrics, review)) = compared else {
            self.transition_reasons.push("\"relief captures missing\"".into());
            return;
        };
        if off_micro != 0 {
            self.transition_reasons.push(format!("\"shaded off drew {off_micro} micro triangles\""));
        }
        if on_micro == 0 || on_micro > 18432 {
            self.transition_reasons.push(format!("\"shaded on drew {on_micro} micro triangles\""));
        }
        if on_invalid != 0 || color_invalid != 0 {
            self.transition_reasons.push(format!("\"generated geometry failed {} validity checks\"", on_invalid.saturating_add(color_invalid)));
        }
        if displacement_um > 8000 {
            self.transition_reasons.push(format!("\"displacement {displacement_um} um exceeds 8000\""));
        }
        if base_triangles == 0 || leaf_triangles != 6_122_214 {
            self.transition_reasons.push(format!("\"base surface {base_triangles} leaf {leaf_triangles}\""));
        }
        if color_fingerprint != on_fingerprint || color_micro != on_micro {
            self.transition_reasons.push("\"color debug does not match the shaded micro mesh\"".into());
        }
        println!(
            "RFC0002_TRANSITION_RELIEF mean={:.3} changed={:.4} marks={} overlap={:.3} outside={:.3} holes={} solid={} hole_ratio={:.4}",
            metrics.mean_diff,
            metrics.changed_fraction,
            metrics.marks,
            metrics.overlap,
            metrics.outside_diff,
            metrics.holes,
            metrics.solid,
            metrics.hole_ratio
        );
        if metrics.mean_diff < 0.35 || metrics.mean_diff > 8.0 {
            self.transition_reasons.push(format!("\"shaded relief mean diff {:.3} is outside 0.35..8\"", metrics.mean_diff));
        }
        if metrics.changed_fraction < 0.001 || metrics.changed_fraction > 0.20 {
            self.transition_reasons.push(format!("\"shaded relief covers {:.4} of the shop\"", metrics.changed_fraction));
        }
        if metrics.marks < 40 {
            self.transition_reasons.push(format!("\"color debug marked only {} shop pixels\"", metrics.marks));
        }
        if metrics.overlap < 0.20 {
            self.transition_reasons.push(format!("\"only {:.3} of the color marks sit on shaded relief\"", metrics.overlap));
        }
        if metrics.outside_diff > metrics.mean_diff + 0.25 {
            self.transition_reasons.push(format!("\"relief diff outside the marks {:.3} exceeds the shop mean {:.3}\"", metrics.outside_diff, metrics.mean_diff));
        }
        if metrics.solid >= 64 && metrics.hole_ratio > 0.02 {
            self.transition_reasons.push(format!("\"silhouette hole ratio {:.4} exceeds 0.02\"", metrics.hole_ratio));
        }
        if let Some(review) = review {
            let _ = write_png(&format!("{directory}/review.png"), &review);
            println!("RFC0002_TRANSITION_REVIEW {directory}/review.png");
        }
    }

    fn record_transition_frame(&mut self) {
        let stats = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats());
        let distance = self.transition_distances.get(self.transition_index).copied().unwrap_or(0.0);
        let row = TransitionRow {
            distance,
            projected_px: self.shop_detail_px().unwrap_or(0.0),
            base_triangles: stats.map(|stats| stats.hierarchy_lod_triangles).unwrap_or(0),
            micro_triangles: stats.map(|stats| stats.micro_triangles).unwrap_or(0),
            vertices: stats.map(|stats| stats.micro_vertices).unwrap_or(0),
            generation_us: stats.map(|stats| stats.micro_generation_us).unwrap_or(0),
            upload_us: stats.map(|stats| stats.micro_upload_us).unwrap_or(0),
            reused: stats.map(|stats| stats.micro_reused).unwrap_or(0),
            fingerprint: stats.map(|stats| stats.micro_fingerprint).unwrap_or(0),
            invalid: stats.map(|stats| stats.micro_invalid).unwrap_or(0),
            leaf_triangles: stats.map(|stats| stats.hierarchy_leaf_triangles).unwrap_or(0),
        };
        let leg = if self.transition_index < self.transition_forward { "out" } else { "back" };
        println!(
            "RFC0002_TRANSITION_FRAME {leg} distance={:.3} projected={:.3} base={} micro={} verts={} gen_us={} upload_us={} queue_us={} publish_us={} reuse={} cancelled={} stale={} partial={} fingerprint={:x} leaves={}",
            row.distance,
            row.projected_px,
            row.base_triangles,
            row.micro_triangles,
            row.vertices,
            row.generation_us,
            row.upload_us,
            stats.map(|stats| stats.micro_queue_wait_us).unwrap_or(0),
            stats.map(|stats| stats.micro_publish_us).unwrap_or(0),
            row.reused,
            stats.map(|stats| stats.micro_cancelled).unwrap_or(0),
            stats.map(|stats| stats.micro_stale_discarded).unwrap_or(0),
            stats.map(|stats| stats.micro_partial).unwrap_or(0),
            row.fingerprint,
            row.leaf_triangles
        );
        if leg == "out" && (0.55..1.45).contains(&row.projected_px) {
            if let Ok(image) = capture_window_image(self.frame) {
                let directory = self.evidence_directory();
                let _ = std::fs::create_dir_all(&directory);
                let _ = write_png(&format!("{directory}/boundary-{:02}-{:.2}.png", self.transition_index, row.projected_px), &image);
                let (origin_x, origin_y, view_w, view_h) = viewport_origin(self.frame, self.panel_hwnd(PERSPECTIVE));
                let region = shop_region(self.shop_projection().as_ref(), origin_x, origin_y, view_w, view_h, image.width, image.height);
                let (crop_w, crop_h, crop) = crop_rgba(&image.rgba, image.width, region);
                let (scaled_w, scaled_h, scaled) = scale_nearest(&crop, crop_w, crop_h, 160, 120);
                self.transition_crops.push((row.projected_px, scaled, scaled_w, scaled_h));
            }
        }
        self.transition_rows.push(row);
    }

    fn finish_transition(&mut self) {
        let directory = self.evidence_directory();
        let _ = std::fs::create_dir_all(&directory);
        let mut lines = vec!["leg,distance,projected_error,base_tris,micro_tris,generated_verts,generation_us,upload_us,reuse,fingerprint".to_string()];
        for (index, row) in self.transition_rows.iter().enumerate() {
            let leg = if index < self.transition_forward { "out" } else { "back" };
            lines.push(format!(
                "{leg},{:.4},{:.4},{},{},{},{},{},{},{:x}",
                row.distance, row.projected_px, row.base_triangles, row.micro_triangles, row.vertices, row.generation_us, row.upload_us, row.reused, row.fingerprint
            ));
        }
        let _ = std::fs::write(format!("{directory}/frames.csv"), lines.join("\n"));
        let forward = self.transition_forward.min(self.transition_rows.len());
        let outbound = &self.transition_rows[..forward];
        for pair in outbound.windows(2) {
            if pair[1].micro_triangles > pair[0].micro_triangles {
                self.transition_reasons.push(format!(
                    "\"micro triangles rose from {} to {} while projected error fell from {:.2} to {:.2}\"",
                    pair[0].micro_triangles, pair[1].micro_triangles, pair[0].projected_px, pair[1].projected_px
                ));
            }
        }
        if outbound.iter().any(|row| row.projected_px <= 0.55 && row.micro_triangles != 0) {
            self.transition_reasons.push("\"micro triangles remained below the 1 px threshold\"".into());
        }
        if outbound.iter().any(|row| row.projected_px >= 2.2 && row.micro_triangles == 0) {
            self.transition_reasons.push("\"micro triangles were absent while projected error was above 2.2 px\"".into());
        }
        let plateau: Vec<u32> = outbound.iter().filter(|row| row.projected_px >= 2.2).map(|row| row.micro_triangles).collect();
        if plateau.len() >= 2 && plateau.iter().any(|count| *count != plateau[0]) {
            self.transition_reasons.push("\"the close plateau did not keep one micro triangle count\"".into());
        }
        let back_start = forward;
        let back_len = self.transition_rows.len().saturating_sub(back_start);
        for k in 0..back_len {
            let outbound_index = forward.saturating_sub(2).saturating_sub(k);
            let Some(outbound_row) = self.transition_rows.get(outbound_index) else { continue };
            let Some(return_row) = self.transition_rows.get(back_start + k) else { continue };
            if return_row.fingerprint != outbound_row.fingerprint || return_row.micro_triangles != outbound_row.micro_triangles {
                self.transition_reasons.push(format!(
                    "\"return at {:.2} m fingerprint {:x}/{} did not match outbound {:x}/{}\"",
                    return_row.distance, return_row.fingerprint, return_row.micro_triangles, outbound_row.fingerprint, outbound_row.micro_triangles
                ));
                break;
            }
        }
        for index in 1..self.transition_rows.len() {
            let previous = &self.transition_rows[index - 1];
            let row = &self.transition_rows[index];
            if row.fingerprint == previous.fingerprint && row.micro_triangles == previous.micro_triangles && (row.reused != 1 || row.generation_us != 0 || row.upload_us != 0) {
                self.transition_reasons.push(format!(
                    "\"frame {index} repeated fingerprint {:x} but regenerated gen {} upload {} reuse {}\"",
                    row.fingerprint, row.generation_us, row.upload_us, row.reused
                ));
                break;
            }
            if row.base_triangles < 500_000 {
                self.transition_reasons.push(format!("\"frame {index} base surface fell to {} triangles\"", row.base_triangles));
                break;
            }
            if index + 1 < self.transition_rows.len() {
                let next = &self.transition_rows[index + 1];
                let dipped = row.base_triangles * 100 < previous.base_triangles * 55;
                let recovered = next.base_triangles * 100 > row.base_triangles * 140 && next.base_triangles * 100 > previous.base_triangles * 80;
                if dipped && recovered {
                    self.transition_reasons.push(format!("\"frame {index} dropped the base surface for one frame\""));
                    break;
                }
            }
            if row.invalid != 0 {
                self.transition_reasons.push(format!("\"frame {index} had {} invalid micro triangles\"", row.invalid));
                break;
            }
        }
        let reused = self.transition_rows.iter().filter(|row| row.reused == 1).count();
        if reused < 3 {
            self.transition_reasons.push(format!("\"only {reused} frames reused an unchanged patch set\""));
        }
        let end_seal = self.townshop_seal();
        match (self.micro_seal.as_ref(), end_seal.as_ref()) {
            (Some(start), Some(end)) => {
                if start.vertices != end.vertices || start.bytes != end.bytes || start.hash != end.hash {
                    self.transition_reasons.push("\"source mesh bytes changed\"".into());
                }
                if start.leaf_triangles != end.leaf_triangles || start.hierarchy_nodes != end.hierarchy_nodes || start.leaf_triangles != 6_122_214 {
                    self.transition_reasons.push("\"RFC-0001 hierarchy changed\"".into());
                }
                if start.parents_len != end.parents_len || start.parents_modified != end.parents_modified {
                    self.transition_reasons.push("\"jarvigparents changed\"".into());
                }
            }
            _ => self.transition_reasons.push("\"source seal missing\"".into()),
        }
        if let Some(strip) = compose_transition_strip(&self.transition_crops) {
            let _ = write_png(&format!("{directory}/transition-strip.png"), &strip);
            println!("RFC0002_TRANSITION_STRIP {directory}/transition-strip.png");
        }
        let passed = self.transition_reasons.is_empty();
        let verdict = if passed { "PASS" } else { "FAIL" };
        let reasons_json = format!("[{}]", self.transition_reasons.join(","));
        let max_gen = self.transition_rows.iter().map(|row| row.generation_us).max().unwrap_or(0);
        let max_upload = self.transition_rows.iter().map(|row| row.upload_us).max().unwrap_or(0);
        let json = format!(
            "{{\"verdict\":\"RFC0002_TRANSITION_{verdict}\",\"frames\":{},\"reused_frames\":{reused},\"max_generation_us\":{max_gen},\"max_upload_us\":{max_upload},\"reasons\":{reasons_json}}}",
            self.transition_rows.len()
        );
        let _ = std::fs::write(format!("{directory}/report.json"), json);
        println!("RFC0002_TRANSITION_{verdict} {directory}");
        if self.rfc0002_refine {
            println!("RFC0002_REFINE_{verdict} {directory}");
        }
        println!("RFC0002_TRANSITION_REASONS {reasons_json}");
        self.finish_lod_capture();
    }

    fn toggle_einstein_debug(&mut self) {
        self.einstein_debug = !self.einstein_debug;
        self.sync_view_menu();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_einstein_debug(self.einstein_debug, self.einstein_seed);
        }
        self.append(if self.einstein_debug {
            "Einstein detail debug is on. A coarse projected error keeps the ordinary material. A fine error draws the hat sample on the same surface. The hierarchy is unchanged."
        } else {
            "Einstein detail debug is off. The ordinary material is drawn."
        });
        self.refresh_status();
    }

    fn ensure_einstein_uvs(&mut self) {
        if !self.einstein_uvs.is_empty() {
            return;
        }
        let Some(entity) = self.find_named_entity("townshop") else { return };
        let Ok(snapshot) = self.engine.world().extract(jarvig_core::RenderFrameId(77)) else { return };
        let Some(instance) = snapshot.instances().iter().find(|instance| instance.entity == entity) else { return };
        let Some(mesh) = self.engine.world().meshes().get(instance.mesh) else { return };
        let Some(stream) = mesh.streams().first() else { return };
        if stream.stride != 60 {
            return;
        }
        let count = stream.bytes.len() / 60;
        let step = (count / 256).max(1);
        let mut uvs = Vec::new();
        for index in (0..count).step_by(step) {
            let offset = index * 60 + 24;
            let Some(bytes) = stream.bytes.get(offset..offset + 8) else { break };
            let u = f32::from_le_bytes(bytes[0..4].try_into().unwrap_or([0; 4]));
            let v = f32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
            if u.is_finite() && v.is_finite() {
                uvs.push([u, v]);
            }
            if uvs.len() == 256 {
                break;
            }
        }
        self.einstein_uvs = uvs;
    }

    fn shop_detail_px(&self) -> Option<f32> {
        let target = self.shop_target()?;
        let camera = self.editor_camera.as_ref()?;
        let forward = camera.forward();
        let dx = target.center.x - camera.position.x;
        let dy = target.center.y - camera.position.y;
        let dz = target.center.z - camera.position.z;
        let depth = (dx * forward.x + dy * forward.y + dz * forward.z).max(0.05) as f32;
        let tan_y = (camera.vertical_fov_radians * 0.5).tan() as f32;
        Some(jarvig_core::projected_detail_px(jarvig_core::DETAIL_FEATURE_SIZE_M, depth, self.viewport_px.1.max(1) as f32, tan_y))
    }

    fn capture_einstein_shot(&mut self, name: &str) -> EinsteinCapture {
        let projected = self.shop_detail_px().unwrap_or(0.0);
        let probe = jarvig_core::probe_detail(&self.einstein_uvs, self.einstein_seed, projected, self.einstein_debug);
        let stats = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats());
        let draws = stats.map(|stats| stats.einstein_debug_draws).unwrap_or(0);
        let triangles = stats.map(|stats| stats.hierarchy_lod_triangles).unwrap_or(0);
        let leaves = stats.map(|stats| stats.hierarchy_leaves).unwrap_or(0);
        let directory = format!("{}/../target/rfc0002", env!("CARGO_MANIFEST_DIR"));
        let _ = std::fs::create_dir_all(&directory);
        let mut rgba = Vec::new();
        let mut width = 0;
        let mut height = 0;
        let mut region = (0, 0, 1, 1);
        let mut palette = 0.0;
        if let Ok(image) = capture_window_image(self.frame) {
            let _ = write_png(&format!("{directory}/{name}.png"), &image);
            let (origin_x, origin_y, view_w, view_h) = viewport_origin(self.frame, self.panel_hwnd(PERSPECTIVE));
            region = shop_region(self.shop_projection().as_ref(), origin_x, origin_y, view_w, view_h, image.width, image.height);
            palette = saturated_fraction(&image.rgba, image.width, region);
            rgba = image.rgba;
            width = image.width;
            height = image.height;
        }
        println!(
            "RFC0002_SHOT {name} palette={palette:.3} samples={} ordinary={} einstein={} gen_us={} draws={draws} tris={triangles} leaves={leaves} projected={projected:.2} image={width}x{height}",
            probe.samples, probe.ordinary, probe.einstein, probe.generation_us
        );
        EinsteinCapture { palette, probe, draws, triangles, leaves, projected_px: projected, rgba, width, height, region }
    }

    fn finish_rfc0002(&mut self) {
        let directory = format!("{}/../target/rfc0002", env!("CARGO_MANIFEST_DIR"));
        let _ = std::fs::create_dir_all(&directory);
        let mut reasons = Vec::new();
        let (off, on, again, orbit, far) = match (
            self.rfc0002_off.as_ref(),
            self.rfc0002_on.as_ref(),
            self.rfc0002_again.as_ref(),
            self.rfc0002_orbit.as_ref(),
            self.rfc0002_far.as_ref(),
        ) {
            (Some(off), Some(on), Some(again), Some(orbit), Some(far)) => (off, on, again, orbit, far),
            _ => {
                println!("RFC0002_FAIL missing captures");
                self.finish_lod_capture();
                return;
            }
        };
        if off.draws != 0 {
            reasons.push(format!("\"debug off submitted {} overlay draws\"", off.draws));
        }
        if off.probe.einstein != 0 {
            reasons.push(format!("\"debug off classified {} einstein samples\"", off.probe.einstein));
        }
        if off.palette > 0.08 {
            reasons.push(format!("\"debug off shop saturation {:.3} is above 0.08\"", off.palette));
        }
        if on.draws == 0 {
            reasons.push("\"debug on submitted no overlay\"".into());
        }
        if on.projected_px <= 1.0 {
            reasons.push(format!("\"close projected error {:.2} did not cross 1 px\"", on.projected_px));
        }
        if on.probe.einstein == 0 {
            reasons.push("\"close debug classified no einstein samples\"".into());
        }
        if on.palette < 0.20 {
            reasons.push(format!("\"close debug saturation {:.3} is under 0.20\"", on.palette));
        }
        if on.triangles != off.triangles || on.leaves != off.leaves {
            reasons.push(format!("\"hierarchy cut changed when debug turned on {}/{} vs {}/{}\"", on.leaves, on.triangles, off.leaves, off.triangles));
        }
        if on.probe.fingerprint != again.probe.fingerprint {
            reasons.push("\"repeat capture changed the einstein fingerprint\"".into());
        }
        let delta = mean_abs_diff(&on.rgba, &again.rgba, on.width, on.region, again.region);
        if delta > 6.0 {
            reasons.push(format!("\"repeat capture pixel delta {delta:.2} is above 6\""));
        }
        if orbit.probe.fingerprint != on.probe.fingerprint {
            reasons.push("\"orbit changed the einstein fingerprint\"".into());
        }
        if far.projected_px > 1.0 {
            reasons.push(format!("\"far projected error {:.2} stayed above 1 px\"", far.projected_px));
        }
        if far.probe.einstein != 0 {
            reasons.push(format!("\"far debug classified {} einstein samples\"", far.probe.einstein));
        }
        if far.palette > 0.08 {
            reasons.push(format!("\"far debug saturation {:.3} is above 0.08\"", far.palette));
        }
        let passed = reasons.is_empty();
        let verdict = if passed { "PASS" } else { "FAIL" };
        let reasons_json = format!("[{}]", reasons.join(","));
        let json = format!(
            "{{\"verdict\":\"RFC0002_{verdict}\",\"seed\":{},\"threshold_px\":{:.1},\"off\":{{\"palette\":{:.4},\"einstein\":{},\"draws\":{},\"triangles\":{},\"projected_px\":{:.2}}},\"close\":{{\"palette\":{:.4},\"einstein\":{},\"ordinary\":{},\"draws\":{},\"triangles\":{},\"projected_px\":{:.2},\"generation_us\":{}}},\"repeat_delta\":{delta:.3},\"orbit_fingerprint_match\":{},\"far\":{{\"palette\":{:.4},\"einstein\":{},\"projected_px\":{:.2}}},\"reasons\":{reasons_json}}}",
            self.einstein_seed,
            jarvig_core::DETAIL_ERROR_THRESHOLD_PX,
            off.palette,
            off.probe.einstein,
            off.draws,
            off.triangles,
            off.projected_px,
            on.palette,
            on.probe.einstein,
            on.probe.ordinary,
            on.draws,
            on.triangles,
            on.projected_px,
            on.probe.generation_us,
            orbit.probe.fingerprint == on.probe.fingerprint,
            far.palette,
            far.probe.einstein,
            far.projected_px
        );
        let _ = std::fs::write(format!("{directory}/report.json"), json);
        println!("RFC0002_{verdict} {directory}");
        println!("RFC0002_REASONS {reasons_json}");
        self.finish_lod_capture();
    }

    fn einstein_status(&self) -> String {
        if !self.einstein_debug || self.einstein_uvs.is_empty() {
            return String::new();
        }
        let projected = self.shop_detail_px().unwrap_or(0.0);
        let probe = jarvig_core::probe_detail(&self.einstein_uvs, self.einstein_seed, projected, true);
        format!(
            " microdetail samples {} | ordinary {} | einstein {} | threshold {:.1} px | seed {} | gen {} us | projected {:.2} px",
            probe.samples, probe.ordinary, probe.einstein, jarvig_core::DETAIL_ERROR_THRESHOLD_PX, self.einstein_seed, probe.generation_us, projected
        )
    }

    fn sync_parent_geometry(&mut self) {
        if !self.cluster_hierarchy {
            return;
        }
        let Some(entity) = self.selection.primary_entity() else {
            self.append("Select townshop, then turn Cluster Hierarchy on.");
            return;
        };
        let asset = self.engine.world().authored_mesh(entity).and_then(|(_, _, _, _, _, _, mesh, _)| match mesh {
            jarvig_core::MeshAssetRef::Asset { id, .. } => Some(id),
            _ => None,
        });
        let Some(asset) = asset else {
            self.append("This actor has no imported mesh. Parent geometry was not built.");
            return;
        };
        let mesh_id = self.viewed_world().extract(jarvig_core::RenderFrameId(1)).ok().and_then(|snapshot| {
            snapshot.instances().iter().find(|instance| instance.entity == entity).map(|instance| instance.mesh)
        });
        let Some(mesh_id) = mesh_id else {
            self.append("Townshop is not in this view yet. Parent geometry was not built.");
            return;
        };
        if self.renderer.as_ref().and_then(|renderer| renderer.parent_geometry_mesh()) == Some(mesh_id) {
            self.append("Parent geometry is already built for this mesh.");
            return;
        }
        let stride = self.engine.mesh_asset_library().get(asset).and_then(|mesh| mesh.streams().first().map(|stream| stream.stride));
        if stride != Some(60) {
            self.append("Parent geometry needs the 60-byte vertex layout. The draw stays on the leaves.");
            return;
        }
        if self.parent_job.as_ref().is_some_and(|job| !job.finished) {
            self.append("The parent-mesh job is already queued. Leaf meshlets stay on screen.");
            return;
        }
        let captured = {
            let library = self.engine.mesh_asset_library();
            match (library.get(asset), library.meshlets(asset)) {
                (Some(mesh), Some(meshlets)) => Some((mesh.clone(), meshlets.clone(), meshlets.source_fingerprint.clone())),
                _ => None,
            }
        };
        let Some((mesh, meshlets, fingerprint)) = captured else {
            self.append("This actor has no stored meshlets. Parent geometry was not built.");
            return;
        };
        let cache_key = format!("{}:{fingerprint}:parent-mesh-v{}", asset.0, jarvig_core::PARENT_BUILDER_VERSION);
        let cache_path = self.parent_cache_path(asset);
        let id = self.jobs.submit(
            jarvig_core::JobDesc {
                name: "RFC-0001 / townshop / Build Parent Mesh LODs".into(),
                asset: Some(asset),
                cache_key: Some(cache_key),
                dependencies: Vec::new(),
            },
            move |ctx| {
                if let Some(path) = &cache_path {
                    if let Some(loaded) = jarvig_core::load_parent_cache(path, &fingerprint, jarvig_core::MESHLET_BUILDER_VERSION) {
                        ctx.report(1.0, "Loaded parent meshes from the sidecar");
                        return Ok(loaded);
                    }
                }
                let geometry = jarvig_core::build_parent_geometry_with(&mesh, &meshlets, &|| ctx.cancel_requested(), &|fraction, stage| ctx.report(fraction, stage))?;
                if let Some(path) = &cache_path {
                    ctx.report(1.0, "Writing the parent sidecar");
                    if let Err(error) = jarvig_core::save_parent_cache(path, &fingerprint, jarvig_core::MESHLET_BUILDER_VERSION, &geometry) {
                        ctx.report(1.0, &format!("Parent meshes ready. Sidecar was not written: {error}"));
                    }
                }
                Ok(geometry)
            },
        );
        self.parent_job = Some(ParentJob { id, mesh: mesh_id, last_percent: 0, last_state: jarvig_core::JobState::Queued, finished: false });
        self.job_line = "RFC-0001 / townshop / Build Parent Mesh LODs Queued 0% Queued 00:00".into();
        self.append("Queued RFC-0001 / townshop / Build Parent Mesh LODs. The editor keeps drawing the leaf meshlets. View > Cancel Background Job stops it. View > Background Jobs lists the queue.");
        self.refresh_status();
    }

    fn queue_asset_registry(&mut self) {
        let Some(project_file) = self.project_file.clone() else { return };
        let Ok(project) = jarvig_core::load_project_file(&project_file) else { return };
        let Some(root) = project_file.parent().map(|path| path.to_path_buf()) else { return };
        let roots = jarvig_core::RegistryRoots {
            content: project.content_directory,
            config: project.config_directory,
            intermediate: project.intermediate_directory,
        };
        if let Some(previous) = self.registry_job.take() {
            self.jobs.cancel(previous);
        }
        let id = self.jobs.submit(
            jarvig_core::JobDesc {
                name: "Scan Project Content".into(),
                asset: None,
                cache_key: Some("asset-registry-v1".into()),
                dependencies: Vec::new(),
            },
            move |ctx| {
                ctx.report(0.2, "Loading asset registry");
                let registry = jarvig_core::reconcile_asset_registry(&root, &roots)?;
                ctx.report(1.0, "Ready");
                Ok(registry)
            },
        );
        self.registry_job = Some(id);
        self.append("Queued project asset registry. The content browser fills in when the scan finishes.");
    }

    fn poll_registry_job(&mut self) {
        let Some(id) = self.registry_job else { return };
        let Some(snap) = self.jobs.snapshots().into_iter().find(|snap| snap.id == id) else { return };
        if !snap.state.finished() {
            return;
        }
        self.registry_job = None;
        if snap.state != jarvig_core::JobState::Completed {
            self.append("Project asset scan did not finish. The content browser stays empty.");
            return;
        }
        let Some(Ok(registry)) = self.jobs.take_result::<jarvig_core::AssetRegistry>(id) else { return };
        let count = registry.assets.len();
        self.browser.assets = registry.assets;
        self.browser.load_us = registry.load_us;
        self.browser.scan_us = registry.scan_us;
        self.browser.reused = registry.reused;
        self.browser.from_cache = registry.from_cache;
        self.attach_resident_previews();
        let line = format!(
            "ASSET_REGISTRY assets={count} reused={} changed={} load_us={} scan_us={} cache={} thumb_bytes={}",
            self.browser.reused, registry.changed, self.browser.load_us, self.browser.scan_us, u32::from(self.browser.from_cache), self.browser.thumbnail_bytes
        );
        println!("{line}");
        self.append(&line);
        if self.content_check {
            self.finish_content_check();
        }
        unsafe { InvalidateRect(self.panel_hwnd(CONTENT), std::ptr::null(), 1); }
        self.refresh_status();
    }

    fn finish_content_check(&mut self) {
        let mut reasons = Vec::new();
        if !self.browser.assets.iter().any(|asset| asset.kind == "Model" && asset.name == "townshop") {
            reasons.push("townshop was not listed".to_string());
        }
        if !self.browser.assets.iter().any(|asset| asset.kind == "Level") {
            reasons.push("startup level was not listed".to_string());
        }
        if self.browser.assets.iter().any(|asset| asset.path.contains("Intermediate") || asset.path.contains("Saved") || asset.path.ends_with("catalog.jarvigassets")) {
            reasons.push("a generated or internal file was listed".to_string());
        }
        if self.browser.thumbnail_bytes == 0 {
            reasons.push("no thumbnail bytes were cached".to_string());
        }
        if let Some(asset) = self.browser.assets.iter().find(|asset| asset.kind == "Model" && asset.name == "townshop").cloned() {
            match self.engine.place_existing_mesh(asset.id, 1.0, 0.0, 1.0) {
                Ok(entity) => {
                    self.content_undo.push(vec![entity]);
                    let before = self.engine.world().entity_count();
                    self.undo_content_drop();
                    if self.engine.world().entity_count() != before.saturating_sub(1) && self.content_undo.is_empty() {
                        reasons.push(format!("undo left {} entities", self.engine.world().entity_count()));
                    }
                }
                Err(error) => reasons.push(format!("place failed: {error}")),
            }
        }
        if reasons.is_empty() {
            println!(
                "CONTENT_BROWSER_PASS assets={} reused={} scan_us={} load_us={} thumb_bytes={} cache={}",
                self.browser.assets.len(),
                self.browser.reused,
                self.browser.scan_us,
                self.browser.load_us,
                self.browser.thumbnail_bytes,
                u32::from(self.browser.from_cache)
            );
        } else {
            println!("CONTENT_BROWSER_FAIL {}", reasons.join("; "));
        }
        self.finish_lod_capture();
    }

    fn attach_resident_previews(&mut self) {
        let previews: Vec<(jarvig_core::AssetId, u32, u32, Vec<u8>)> = self
            .engine
            .mesh_asset_library()
            .records()
            .iter()
            .filter_map(|record| {
                let texture = self.engine.mesh_asset_library().imported_surface(record.id)?.base_color.as_ref()?;
                let (width, height, rgba) = jarvig_core::thumbnail_rgba(texture.width(), texture.height(), texture.pixels().to_vec());
                Some((record.id, width, height, rgba))
            })
            .collect();
        for (id, width, height, rgba) in previews {
            if self.browser.thumb(id).is_some() {
                continue;
            }
            self.browser.thumbnail_bytes = self.browser.thumbnail_bytes.saturating_add(rgba.len() as u64);
            self.browser.thumbs.push(content_browser::Thumb { id, width, height, rgba });
        }
    }

    fn cursor_over_viewport(&self) -> Option<(f64, f64)> {
        let hwnd = self.panel_hwnd(PERSPECTIVE);
        if hwnd.is_null() {
            return None;
        }
        let mut point = POINT { x: 0, y: 0 };
        unsafe {
            if GetCursorPos(&mut point) == 0 || ScreenToClient(hwnd, &mut point) == 0 {
                return None;
            }
            let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            GetClientRect(hwnd, &mut rect);
            if point.x < 0 || point.y < 0 || point.x >= rect.right || point.y >= rect.bottom {
                return None;
            }
        }
        Some((point.x as f64, point.y as f64))
    }

    fn update_content_drag(&mut self) {
        let Some(id) = self.content_drag else { return };
        let Some(asset) = self.browser.assets.iter().find(|asset| asset.id == id).cloned() else { return };
        self.drop_feedback = content_browser::drop_caption(&asset, self.cursor_over_viewport().is_some());
        self.refresh_status();
    }

    fn drop_content_asset(&mut self) {
        let Some(id) = self.content_drag.take() else { return };
        let Some(asset) = self.browser.assets.iter().find(|asset| asset.id == id).cloned() else { return };
        let over = self.cursor_over_viewport();
        self.drop_feedback.clear();
        let caption = content_browser::drop_caption(&asset, over.is_some());
        if !content_browser::drop_places_actor(&asset, over.is_some()) {
            self.append(&caption);
            self.refresh_status();
            return;
        }
        let Some((x, y)) = over else { return };
        let Some(ray) = self.viewport_ray(x, y) else {
            self.append("The viewport has no camera yet.");
            return;
        };
        let hit = content_ground_hit(&ray);
        if asset.kind == "Prefab" {
            self.place_prefab_asset(&asset, jarvig_core::Vec3::new(hit.x, hit.y, hit.z));
            return;
        }
        match self.engine.place_existing_mesh(asset.id, hit.x, hit.y, hit.z) {
            Ok(entity) => {
                self.content_undo.push(vec![entity]);
                if let Ok(item) = crate::selection::SelectionItem::entity(entity) {
                    let _ = self.selection.replace(item);
                }
                self.sync_outliner();
                self.refresh_title();
                self.append(&format!("Placed {} at {:.2}, {:.2}, {:.2}. Ctrl+Z removes that actor.", asset.name, hit.x, hit.y, hit.z));
            }
            Err(error) => self.append(&format!("Mesh was not placed. {error}")),
        }
        self.refresh_status();
    }

    fn place_prefab_asset(&mut self, asset: &jarvig_core::RegistryAsset, hit: jarvig_core::Vec3) {
        let Some(project_file) = self.project_file.clone() else {
            self.append("No project is open.");
            self.refresh_status();
            return;
        };
        let Some(root) = project_file.parent() else {
            self.append("The project file has no directory.");
            self.refresh_status();
            return;
        };
        match self.engine.instantiate_prefab_file(&root.join(&asset.path), hit) {
            Ok(entities) => {
                let count = entities.len();
                if let Some(root_entity) = entities.first().copied() {
                    if let Ok(item) = crate::selection::SelectionItem::entity(root_entity) {
                        let _ = self.selection.replace(item);
                    }
                }
                self.content_undo.push(entities);
                self.sync_outliner();
                self.refresh_title();
                self.append(&format!(
                    "Placed {} ({} parts) at {:.2}, {:.2}, {:.2}. Ctrl+Z removes that actor.",
                    asset.name, count, hit.x, hit.y, hit.z
                ));
            }
            Err(error) => self.append(&format!("Prefab was not placed. {error}")),
        }
        self.refresh_status();
    }

    fn undo_content_drop(&mut self) {
        let Some(group) = self.content_undo.pop() else {
            self.append("No mesh placement to undo.");
            return;
        };
        let count = group.len();
        for entity in group.into_iter().rev() {
            if let Err(error) = self.engine.execute_authoring(AuthoringCommand::DestroyEntity { target: entity }) {
                self.append(&format!("Undo failed: {error}."));
                self.sync_outliner();
                self.refresh_title();
                return;
            }
        }
        self.append(&format!("Undid placement of {count} actor(s)."));
        self.sync_outliner();
        self.refresh_title();
    }

    fn select_character_root(&mut self) {
        let outline = self.engine.world().entity_outline();
        let jointed = |row: &jarvig_core::EntityOutlineInfo| self.engine.world().authored_joint(row.uuid).is_some();
        let root = outline.iter().find(|row| row.name == "Pelvis" && jointed(row)).map(|row| row.uuid).or_else(|| {
            outline.iter().find(|row| row.parent.is_none() && jointed(row)).map(|row| row.uuid)
        }).or_else(|| outline.iter().find(|row| jointed(row)).map(|row| row.uuid));
        let Some(root) = root else { return };
        if let Ok(item) = crate::selection::SelectionItem::entity(root) {
            let _ = self.selection.replace(item);
        }
        self.focus_selected();
    }

    fn editor_mode(&self) -> chrome::WorkspaceMode {
        if self.character_workspace {
            chrome::WorkspaceMode::Character
        } else if self.land_mode {
            chrome::WorkspaceMode::Land
        } else {
            chrome::WorkspaceMode::Level
        }
    }

    fn level_has_rig(&self) -> bool {
        self.engine.world().entity_outline().iter().any(|row| self.engine.world().authored_joint(row.uuid).is_some())
    }

    fn workspace_line(mode: chrome::WorkspaceMode) -> &'static str {
        match mode {
            chrome::WorkspaceMode::Level => "Level. Characters, props, lights, and terrain stay visible. The terrain grid and rig helpers stay in their own workspaces.",
            chrome::WorkspaceMode::Land => "Land. The grid follows the terrain. Characters stay hidden until Show Full Level. Wheel over the terrain changes brush radius. Einstein detail does not write height.",
            chrome::WorkspaceMode::Character => "Character. The rig is isolated over the same level. Terrain and props are hidden here and stay in the level.",
        }
    }

    fn enter_mode(&mut self, mode: chrome::WorkspaceMode, persist: bool) {
        self.switch_editor_mode(mode, persist, true);
    }

    /// One authored world. `announce` is for a click. Restore and a project template stay quiet.
    fn switch_editor_mode(&mut self, mode: chrome::WorkspaceMode, persist: bool, announce: bool) {
        if mode == chrome::WorkspaceMode::Character && !self.level_has_rig() {
            self.append("Character workspace needs a rig in this level. Level stays on the authored world.");
            self.sync_view_menu();
            return;
        }
        if self.editor_mode() == mode {
            return;
        }
        self.land_stamp = None;
        self.land_hover = None;
        self.land_flatten = None;
        self.outliner_land = false;
        self.land_mode = mode == chrome::WorkspaceMode::Land;
        self.character_workspace = mode == chrome::WorkspaceMode::Character;
        if mode == chrome::WorkspaceMode::Character {
            self.select_character_root();
        }
        if announce {
            self.append(Self::workspace_line(mode));
        }
        self.inspector_force_realize = true;
        self.request_inspector_refresh();
        self.sync_outliner();
        self.sync_view_menu();
        self.refresh_title();
        if persist {
            self.persist_workspace();
        }
        unsafe {
            InvalidateRect(self.dock_host, std::ptr::null(), 1);
            InvalidateRect(self.toolbar, std::ptr::null(), 0);
        }
    }

    fn set_land_mode(&mut self, enabled: bool) {
        self.enter_mode(if enabled { chrome::WorkspaceMode::Land } else { chrome::WorkspaceMode::Level }, true);
    }

    fn realize_land_outliner(&mut self) {
        let hwnd = self.panel_hwnd(OUTLINER);
        if hwnd.is_null() {
            return;
        }
        self.outliner_applying = true;
        self.tree_keys.clear();
        unsafe { SendMessageW(hwnd, TVM_DELETEITEM, 0, TVI_ROOT); }
        let root = self.insert_tree_item(TVI_ROOT, "Land", TreeKey::World);
        for tool in LandTool::ALL {
            self.insert_tree_item(root, tool.label(), TreeKey::Tool(tool));
        }
        if root != 0 {
            unsafe { SendMessageW(hwnd, TVM_EXPAND, TVE_EXPAND as usize, root); }
        }
        let wanted = self.land_tool;
        if let Some(index) = self.tree_keys.iter().position(|key| matches!(key, TreeKey::Tool(tool) if *tool == wanted)) {
            let item = self.find_item_by_param(index as isize);
            if item != 0 {
                unsafe { SendMessageW(hwnd, TVM_SELECTITEM, TVGN_CARET as usize, item); }
            }
        }
        self.outliner_applying = false;
    }

    /// Land Mode shows the terrain form even when the tool row is selected.
    fn inspector_target(&self) -> Option<EntityUuid> {
        if self.land_mode && !self.session_active() {
            if let Some(id) = self.engine.world().terrain_entity() {
                return Some(id);
            }
        }
        if self.selection.count() != 1 {
            return None;
        }
        self.selection.primary_entity()
    }

    fn land_brush(&self) -> Option<jarvig_core::TerrainBrush> {
        if !self.land_mode || self.session_active() {
            return None;
        }
        match self.land_tool {
            LandTool::Sculpt => Some(jarvig_core::TerrainBrush::Sculpt),
            LandTool::Smooth => Some(jarvig_core::TerrainBrush::Smooth),
            LandTool::Flatten => Some(jarvig_core::TerrainBrush::Flatten),
            LandTool::Paint => Some(jarvig_core::TerrainBrush::Paint),
            _ => None,
        }
    }

    fn commit_land_draft(&mut self, binding: &inspector::InspectorBinding, hwnd: HWND) -> bool {
        if !self.land_mode || self.engine.world().terrain_entity().is_some() {
            return false;
        }
        let inspector::InspectorBinding::Text { type_id, field } = binding else { return false };
        if *type_id != jarvig_core::TYPE_TERRAIN {
            return false;
        }
        let text = window_text(hwnd);
        let Some(value) = text.trim().parse::<f64>().ok().filter(|value| value.is_finite()) else {
            self.append("Terrain size stayed. The text was not a finite number.");
            self.realize_inspector_controls();
            return true;
        };
        let meters = value as f32;
        let size = matches!(
            *field,
            jarvig_core::FIELD_TERRAIN_WIDTH | jarvig_core::FIELD_TERRAIN_DEPTH | jarvig_core::FIELD_TERRAIN_SPACING | jarvig_core::FIELD_TERRAIN_CHUNK
        );
        if size && meters <= 0.0 {
            self.append("Terrain size stayed. Width, depth, spacing, and chunk size have to be positive.");
            self.realize_inspector_controls();
            return true;
        }
        match *field {
            jarvig_core::FIELD_TERRAIN_WIDTH => self.land_width = meters,
            jarvig_core::FIELD_TERRAIN_DEPTH => self.land_depth = meters,
            jarvig_core::FIELD_TERRAIN_SPACING => self.land_spacing = meters,
            jarvig_core::FIELD_TERRAIN_CHUNK => self.land_chunk = meters,
            jarvig_core::FIELD_TERRAIN_HEIGHT => self.land_height = meters,
            _ => return false,
        }
        true
    }

    fn create_land_terrain(&mut self) {
        if self.session_active() {
            self.append("Stop play before creating terrain.");
            return;
        }
        let record = match jarvig_core::TerrainRecord::flat(self.land_width, self.land_depth, self.land_spacing, self.land_chunk, self.land_height) {
            Ok(record) => record,
            Err(_) => {
                self.append("Terrain size must divide into whole chunks. Width, depth, and chunk size use the vertex spacing.");
                return;
            }
        };
        let local = self.editor_camera.as_ref().map(|camera| Vec3::new(camera.position.x - jarvig_core::BOOTSTRAP_ROOT_M, 0.0, camera.position.z)).unwrap_or(Vec3::new(0.0, 0.0, 0.0));
        match self.engine.execute_authoring(AuthoringCommand::CreateTerrain { local, record }) {
            Ok(_) => {
                self.evict_retired_meshes();
                if let Some(id) = self.engine.world().terrain_entity() {
                    if let Ok(item) = selection::SelectionItem::entity(id) {
                        let _ = self.selection.replace(item);
                    }
                    self.aim_at_terrain(id);
                }
                self.land_tool = LandTool::Sculpt;
                self.outliner_land = false;
                if self.editor_mode() != chrome::WorkspaceMode::Land {
                    self.enter_mode(chrome::WorkspaceMode::Land, false);
                }
                self.persist_workspace();
                self.append("Created a flat terrain. The heightfield is the ground. Einstein detail is off and does not write height.");
                self.inspector_force_realize = true;
                self.sync_outliner();
                self.refresh_title();
            }
            Err(jarvig_core::AuthoringError::InvalidOperation) => {
                self.append("This world already has a terrain. Select it and sculpt that heightfield.");
            }
            Err(error) => self.append(&format!("Create Terrain failed: {error}")),
        }
    }

    fn aim_at_terrain(&mut self, id: EntityUuid) {
        let Ok(pose) = self.engine.world().entity_world_pose(id) else { return };
        if let Some(camera) = self.editor_camera.as_mut() {
            camera.orbit_pivot = pose.translation;
            camera.orbit_distance = 42.0;
            camera.pitch = -0.65;
            camera.yaw = 0.0;
            camera.speed_m_s = 40.0;
            camera.reorbit();
        }
        let _ = self.push_editor_camera();
    }

    fn select_terrain_actor(&mut self) {
        let Some(id) = self.engine.world().terrain_entity() else {
            self.append("There is no terrain in this world.");
            return;
        };
        if let Ok(item) = selection::SelectionItem::entity(id) {
            let _ = self.selection.replace(item);
        }
        self.sync_selection_view();
    }

    fn log_terrain_debug(&mut self) {
        let Some(id) = self.engine.world().terrain_entity() else {
            self.append("There is no terrain in this world. Einstein detail is not generated.");
            return;
        };
        let Some(record) = self.engine.world().authored_terrain(id) else { return };
        let Ok(pose) = self.engine.world().entity_world_pose(id) else { return };
        let (local_x, local_z) = self
            .editor_camera
            .as_ref()
            .map(|camera| ((camera.position.x - pose.translation.x) as f32, (camera.position.z - pose.translation.z) as f32))
            .unwrap_or((0.0, 0.0));
        self.append(&record.debug_line(local_x, local_z));
    }

    fn land_inspector(&self) -> inspector::LandInspector {
        inspector::LandInspector {
            grid: self.land_grid,
            minor: self.land_grid_minor as f64,
            major: self.land_grid_major as f64,
            snap: self.land_grid_snap,
            radius: self.land_radius as f64,
            strength: self.land_delta as f64,
            falloff: self.land_falloff.label(),
            world: self.land_overlay_world,
            vertices: self.land_overlay_vertices,
            chunks: self.land_overlay_chunks,
            lod: self.land_overlay_lod,
            show_terrain: self.land_show_terrain,
            show_helpers: self.land_show_helpers,
            show_lighting: self.land_show_lighting,
            show_characters: self.land_show_characters,
            show_props: self.land_show_props,
            show_gameplay: self.land_show_gameplay,
            show_full: self.land_show_full,
        }
    }

    fn commit_land_ui(&mut self, binding: &inspector::InspectorBinding, hwnd: HWND) -> bool {
        let inspector::InspectorBinding::Text { field, .. } = binding else { return false };
        if !inspector::is_land_ui(*field) {
            return false;
        }
        let text = window_text(hwnd);
        let Some(value) = text.trim().parse::<f64>().ok().filter(|value| value.is_finite()) else {
            self.append("Land control stayed. The text was not a finite number.");
            self.realize_inspector_controls();
            return true;
        };
        self.set_land_number(*field, value);
        true
    }

    fn set_land_number(&mut self, field: jarvig_core::FieldId, value: f64) {
        let meters = value as f32;
        match field {
            inspector::LAND_UI_MINOR if meters > 0.0 => self.land_grid_minor = meters,
            inspector::LAND_UI_MAJOR if meters > 0.0 => self.land_grid_major = meters.max(self.land_grid_minor),
            inspector::LAND_UI_RADIUS if meters > 0.0 => self.land_radius = meters,
            inspector::LAND_UI_STRENGTH if meters >= 0.0 => self.land_delta = meters,
            _ => {
                self.append("Land control stayed. Spacing, radius, and strength have to be finite and positive.");
                self.realize_inspector_controls();
                return;
            }
        }
        if self.land_grid_major < self.land_grid_minor {
            self.land_grid_major = self.land_grid_minor;
        }
        self.request_inspector_refresh();
        self.persist_workspace();
    }

    fn set_land_check(&mut self, field: jarvig_core::FieldId, checked: bool) {
        match field {
            inspector::LAND_UI_GRID => self.land_grid = checked,
            inspector::LAND_UI_SNAP => self.land_grid_snap = checked,
            inspector::LAND_UI_WORLD => self.land_overlay_world = checked,
            inspector::LAND_UI_VERTS => self.land_overlay_vertices = checked,
            inspector::LAND_UI_CHUNKS => self.land_overlay_chunks = checked,
            inspector::LAND_UI_LOD => self.land_overlay_lod = checked,
            inspector::LAND_UI_SHOW_TERRAIN => self.land_show_terrain = checked,
            inspector::LAND_UI_SHOW_HELPERS => self.land_show_helpers = checked,
            inspector::LAND_UI_SHOW_LIGHTING => self.land_show_lighting = checked,
            inspector::LAND_UI_SHOW_CHARACTERS => self.land_show_characters = checked,
            inspector::LAND_UI_SHOW_PROPS => self.land_show_props = checked,
            inspector::LAND_UI_SHOW_GAMEPLAY => self.land_show_gameplay = checked,
            inspector::LAND_UI_SHOW_FULL => self.land_show_full = checked,
            _ => return,
        }
        self.request_inspector_refresh();
        self.persist_workspace();
    }

    fn set_land_choice(&mut self, field: jarvig_core::FieldId, text: &str) {
        if field == inspector::LAND_UI_FALLOFF {
            if let Some(falloff) = jarvig_core::TerrainFalloff::parse(text) {
                self.land_falloff = falloff;
                self.request_inspector_refresh();
                self.persist_workspace();
                return;
            }
        }
        self.realize_inspector_controls();
    }

    fn land_hidden_entities(&self) -> Vec<jarvig_core::EntityId> {
        if !self.land_mode || self.session_active() || self.land_show_full {
            return Vec::new();
        }
        self.engine
            .world()
            .land_draw_actors()
            .into_iter()
            .filter(|(_, class)| {
                !match class {
                    jarvig_core::LandDrawClass::Terrain => self.land_show_terrain,
                    jarvig_core::LandDrawClass::Character => self.land_show_characters,
                    jarvig_core::LandDrawClass::Prop => self.land_show_props,
                    jarvig_core::LandDrawClass::Gameplay => self.land_show_gameplay,
                }
            })
            .map(|(id, _)| id)
            .collect()
    }

    fn character_hidden_entities(&self) -> Vec<jarvig_core::EntityId> {
        self.engine
            .world()
            .land_draw_actors()
            .into_iter()
            .filter(|(_, class)| *class != jarvig_core::LandDrawClass::Character)
            .map(|(id, _)| id)
            .collect()
    }

    fn publish_land_view(&mut self) {
        if self.renderer.is_none() {
            return;
        }
        if self.session_active() || (!self.land_mode && !self.character_workspace) {
            let renderer = self.renderer.as_mut().expect("renderer");
            renderer.set_terrain_grid(None);
            renderer.set_hidden_entities(&[]);
            renderer.set_land_unlit(false);
            return;
        }
        if self.character_workspace {
            let hidden = self.character_hidden_entities();
            let renderer = self.renderer.as_mut().expect("renderer");
            renderer.set_terrain_grid(None);
            renderer.set_hidden_entities(&hidden);
            renderer.set_land_unlit(false);
            return;
        }
        let hidden = self.land_hidden_entities();
        let unlit = !self.land_show_lighting;
        let grid = self.land_surface_grid();
        let renderer = self.renderer.as_mut().expect("renderer");
        renderer.set_hidden_entities(&hidden);
        renderer.set_land_unlit(unlit);
        renderer.set_terrain_grid(grid);
    }

    fn land_surface_grid(&self) -> Option<TerrainGridDesc> {
        if !self.land_show_helpers {
            return None;
        }
        let lines = self.land_grid;
        let brush_enabled = self.land_brush().is_some() && self.land_hover.is_some();
        let world = lines && self.land_overlay_world;
        let vertices = lines && self.land_overlay_vertices;
        let chunks = lines && self.land_overlay_chunks;
        let lod = lines && self.land_overlay_lod;
        if !world && !vertices && !chunks && !lod && !brush_enabled {
            return None;
        }
        let metrics = self.engine.world().terrain_surface_metrics()?;
        let (brush_x, brush_z) = self.land_hover.unwrap_or((0.0, 0.0));
        Some(TerrainGridDesc {
            meshes: self.engine.world().terrain_surface_meshes(),
            minor_m: self.land_grid_minor,
            major_m: self.land_grid_major.max(self.land_grid_minor),
            world,
            vertices,
            chunks,
            lod,
            lod_enabled: metrics.lod_enabled,
            half_x: metrics.half_x,
            half_z: metrics.half_z,
            vertex_spacing: metrics.spacing_m,
            chunk_m: metrics.chunk_m,
            brush_x,
            brush_z,
            brush_radius: self.land_radius,
            brush_enabled,
            brush_strength: self.land_delta.abs(),
            brush_smooth: self.land_falloff == jarvig_core::TerrainFalloff::Smooth,
            brush_color: self.land_brush_color(),
        })
    }

    fn land_radius_wheel(&self) -> bool {
        self.land_brush().is_some() && self.land_hover.is_some()
    }

    fn scale_land_radius(&mut self, notches: i32) {
        if notches == 0 {
            return;
        }
        let next = (self.land_radius * 1.1_f32.powi(notches)).clamp(0.25, 128.0);
        if (next - self.land_radius).abs() < 1.0e-4 {
            return;
        }
        self.land_radius = next;
        self.request_inspector_refresh();
        self.persist_workspace();
    }

    fn land_brush_color(&self) -> [f32; 4] {
        let shift = (unsafe { GetKeyState(VK_SHIFT as i32) } as u16) & 0x8000 != 0;
        match self.land_tool {
            LandTool::Sculpt if shift => [0.95, 0.42, 0.32, 1.0],
            LandTool::Sculpt => [0.96, 0.86, 0.34, 1.0],
            LandTool::Smooth => [0.42, 0.82, 0.95, 1.0],
            LandTool::Flatten => [0.82, 0.82, 0.82, 1.0],
            LandTool::Paint => [0.45, 0.78, 0.42, 1.0],
            _ => [0.9, 0.9, 0.9, 1.0],
        }
    }

    fn track_land_hover(&mut self, x: f64, y: f64) {
        if !self.land_mode || self.session_active() {
            self.land_hover = None;
            return;
        }
        let Some(id) = self.engine.world().terrain_entity() else {
            self.land_hover = None;
            return;
        };
        let Some(ray) = self.viewport_ray(x, y) else { return };
        self.land_hover = self.engine.world().terrain_ray_local(id, ray.origin, ray.direction).map(|hit| (hit.local_x, hit.local_z));
    }

    fn stamp_land(&mut self, x: f64, y: f64, first: bool) {
        if self.session_active() {
            return;
        }
        let Some(brush) = self.land_brush() else { return };
        let Some(id) = self.engine.world().terrain_entity() else {
            if first {
                self.append("Create a terrain before using this brush.");
            }
            return;
        };
        let Some(ray) = self.viewport_ray(x, y) else { return };
        let Some(hit) = self.engine.world().terrain_ray_local(id, ray.origin, ray.direction) else { return };
        let (local_x, local_z) = if self.land_grid_snap {
            let record = self.engine.world().authored_terrain(id);
            record.map(|record| jarvig_core::snap_terrain_xz(record.width_m, record.depth_m, hit.local_x, hit.local_z, self.land_grid_minor)).unwrap_or((hit.local_x, hit.local_z))
        } else {
            (hit.local_x, hit.local_z)
        };
        self.land_hover = Some((local_x, local_z));
        if let Some((previous_x, previous_z)) = self.land_stamp {
            let dx = local_x - previous_x;
            let dz = local_z - previous_z;
            if !first && dx * dx + dz * dz < 0.25 {
                return;
            }
        }
        self.land_stamp = Some((local_x, local_z));
        if first {
            if let Some(record) = self.engine.world().authored_terrain(id) {
                self.append(&record.debug_line(local_x, local_z));
            }
        }
        let shift = (unsafe { GetKeyState(VK_SHIFT as i32) } as u16) & 0x8000 != 0;
        let delta = if brush == jarvig_core::TerrainBrush::Sculpt && shift { -self.land_delta.abs() } else { self.land_delta.abs() };
        let flatten_to = if brush == jarvig_core::TerrainBrush::Flatten {
            if self.land_flatten.is_none() {
                self.land_flatten = Some(hit.height);
            }
            self.land_flatten
        } else {
            None
        };
        match self.engine.execute_authoring(AuthoringCommand::StampTerrain {
            target: id,
            brush,
            local_x,
            local_z,
            radius: self.land_radius,
            delta,
            layer: self.land_layer,
            falloff: self.land_falloff,
            flatten_to,
            rebuild_meshes: false,
        }) {
            Ok(_) => {
                let dirty = self.engine.world_mut().take_terrain_dirty();
                for coord in dirty {
                    self.bump_land_chunk(coord);
                }
                self.queue_land_chunks();
                self.request_inspector_refresh();
            }
            Err(error) => {
                if first {
                    self.append(&format!("Terrain brush failed: {error}"));
                }
            }
        }
    }

    fn bump_land_chunk(&mut self, coord: jarvig_core::ChunkCoord) {
        if let Some(slot) = self.land_chunk_rev.iter_mut().find(|item| item.0 == coord.x && item.1 == coord.z) {
            slot.2 = slot.2.saturating_add(1);
        } else {
            self.land_chunk_rev.push((coord.x, coord.z, 1));
        }
    }

    fn land_chunk_revision(&self, x: u32, z: u32) -> u64 {
        self.land_chunk_rev.iter().find(|item| item.0 == x && item.1 == z).map(|item| item.2).unwrap_or(0)
    }

    fn note_land_chunk_published(&mut self, x: u32, z: u32, revision: u64) {
        if let Some(slot) = self.land_chunk_published.iter_mut().find(|item| item.0 == x && item.1 == z) {
            slot.2 = revision;
        } else {
            self.land_chunk_published.push((x, z, revision));
        }
    }

    fn cancel_land_chunks(&mut self) {
        self.land_visual_epoch = self.land_visual_epoch.saturating_add(1);
        self.land_hover = None;
        self.land_flatten = None;
        self.land_stamp = None;
        self.land_chunk_rev.clear();
        self.land_chunk_published.clear();
        if let Some(id) = self.land_chunk_job.take() {
            self.jobs.cancel(id);
            let _ = self.jobs.take_result::<LandChunkProduct>(id);
        }
    }

    fn queue_land_chunks(&mut self) {
        if self.land_chunk_job.is_some() {
            return;
        }
        let Some(entity) = self.engine.world().terrain_entity() else { return };
        let revisions_now = self.land_chunk_rev.clone();
        let published = self.land_chunk_published.clone();
        let pending: Vec<jarvig_core::ChunkCoord> = revisions_now
            .into_iter()
            .filter(|item| !published.iter().any(|stored| stored.0 == item.0 && stored.1 == item.1 && stored.2 == item.2))
            .map(|item| jarvig_core::ChunkCoord { x: item.0, z: item.1 })
            .collect();
        if pending.is_empty() {
            return;
        }
        let Some(record) = self.engine.world().authored_terrain(entity) else { return };
        let revisions: Vec<(u32, u32, u64)> = pending.iter().map(|coord| (coord.x, coord.z, self.land_chunk_revision(coord.x, coord.z))).collect();
        let epoch = self.land_visual_epoch;
        let id = self.jobs.submit(
            jarvig_core::JobDesc {
                name: "Land / terrain chunks".into(),
                asset: None,
                cache_key: None,
                dependencies: Vec::new(),
            },
            move |ctx| {
                ctx.report(0.2, "Building");
                if ctx.cancel_requested() {
                    return Err("cancelled".into());
                }
                let built = jarvig_core::build_dirty_chunks(&record, &pending).map_err(|error| error.to_string())?;
                ctx.report(1.0, "Built");
                Ok(LandChunkProduct { epoch, entity, revisions, built })
            },
        );
        self.land_chunk_job = Some(id);
    }

    fn poll_land_chunks(&mut self) {
        let Some(id) = self.land_chunk_job else { return };
        let snap = self.jobs.snapshots().into_iter().find(|snap| snap.id == id);
        let Some(snap) = snap else {
            self.land_chunk_job = None;
            return;
        };
        if !snap.state.finished() {
            let seconds = snap.elapsed_ms / 1000;
            self.job_line = format!("Land / terrain chunks {} {:.0}% {} {:02}:{:02}", snap.state.label(), snap.progress * 100.0, snap.stage, seconds / 60, seconds % 60);
            return;
        }
        self.land_chunk_job = None;
        if self.job_line.starts_with("Land / terrain chunks") {
            self.job_line.clear();
            self.refresh_status();
        }
        if snap.state == jarvig_core::JobState::Completed {
            if let Some(Ok(product)) = self.jobs.take_result::<LandChunkProduct>(id) {
                if product.epoch == self.land_visual_epoch && self.engine.world().terrain_entity() == Some(product.entity) {
                    let mut accepted = Vec::new();
                    let mut notes = Vec::new();
                    for chunk in product.built {
                        let revision = product.revisions.iter().find(|item| item.0 == chunk.coord.x && item.1 == chunk.coord.z).map(|item| item.2);
                        if revision.is_some_and(|revision| revision == self.land_chunk_revision(chunk.coord.x, chunk.coord.z)) {
                            if let Some(revision) = revision {
                                notes.push((chunk.coord.x, chunk.coord.z, revision));
                            }
                            accepted.push((chunk.coord.x, chunk.coord.z, chunk.mesh));
                        }
                    }
                    if !accepted.is_empty() {
                        match self.engine.world_mut().install_terrain_chunks(product.entity, accepted) {
                            Ok(()) => {
                                for (x, z, revision) in notes {
                                    self.note_land_chunk_published(x, z, revision);
                                }
                                self.evict_retired_meshes();
                            }
                            Err(error) => self.append(&format!("Terrain chunk publish failed: {error}")),
                        }
                    }
                }
            }
        } else {
            let _ = self.jobs.take_result::<LandChunkProduct>(id);
        }
        self.queue_land_chunks();
    }

    fn evict_retired_meshes(&mut self) {
        let retired = self.engine.world_mut().take_retired_meshes();
        if let Some(renderer) = self.renderer.as_mut() {
            for mesh in retired {
                let _ = renderer.evict_gpu_mesh(mesh);
            }
        }
    }

    fn set_character_workspace(&mut self, enabled: bool) {
        self.enter_mode(if enabled { chrome::WorkspaceMode::Character } else { chrome::WorkspaceMode::Level }, true);
    }

    fn open_character_asset(&mut self, asset: &jarvig_core::RegistryAsset) {
        self.browser.selected = Some(asset.id);
        if !self.level_has_rig() {
            self.append(&format!("{} was not placed. Drag it into the viewport to instantiate it. Character workspace opens a level that has a rig.", asset.name));
            return;
        }
        self.enter_mode(chrome::WorkspaceMode::Character, true);
        self.append(&format!("Editing {}. The rig workspace hides terrain and props. They stay in the level.", asset.name));
    }

    fn reset_character_pose(&mut self) {
        let pose: Vec<_> = self
            .engine
            .world()
            .entity_outline()
            .iter()
            .filter_map(|row| {
                let joint = self.engine.world().authored_joint(row.uuid)?;
                Some((row.uuid, joint.rest_translation, joint.rest_rotation))
            })
            .collect();
        if pose.is_empty() {
            self.append("Reset Pose found no joints.");
            return;
        }
        let count = pose.len();
        for (id, translation, rotation) in pose {
            let _ = inspector::submit_property(
                &mut self.engine,
                id,
                jarvig_core::TYPE_SPATIAL_FRAME,
                jarvig_core::FIELD_LOCAL_TRANSLATION,
                PropertyValue::Vec3(translation),
            );
            let _ = inspector::submit_property(
                &mut self.engine,
                id,
                jarvig_core::TYPE_SPATIAL_FRAME,
                jarvig_core::FIELD_LOCAL_ROTATION,
                PropertyValue::Quat(rotation),
            );
        }
        self.sync_outliner();
        self.append(&format!("Reset {count} joints to their rest poses."));
    }

    fn submit_micro_request(&mut self) {
        let Some(request) = self.renderer.as_mut().and_then(|renderer| renderer.take_micro_request()) else { return };
        if let Some(previous) = self.micro_job.as_ref() {
            if !previous.finished && !previous.cancel_noted {
                self.jobs.cancel(previous.id);
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.note_micro_cancelled();
                }
            }
        }
        let queued_at = std::time::Instant::now();
        let epoch = request.epoch;
        let key = request.key;
        let skipped = request.skipped;
        let anchors = request.anchors;
        let seed = request.seed;
        let height = request.height;
        let tan_half = request.tan_half;
        let budget = request.budget;
        let provider = request.provider;
        let id = self.jobs.submit(
            jarvig_core::JobDesc {
                name: "RFC-0002 / townshop / Einstein Surface".into(),
                asset: None,
                cache_key: Some(format!("micro:{key:016x}")),
                dependencies: Vec::new(),
            },
            move |ctx| {
                let queue_wait_us = queued_at.elapsed().as_micros().min(u128::from(u32::MAX)) as u32;
                ctx.report(0.05, "Sampling");
                if ctx.cancel_requested() {
                    return Err("cancelled".into());
                }
                ctx.report(0.2, "Building");
                let mesh = jarvig_core::build_procedural_microtriangles_cancellable(&anchors, seed, true, height, tan_half, &budget, provider, &|| ctx.cancel_requested())?;
                Ok(MicroJobProduct { epoch, key, mesh, queue_wait_us, skipped })
            },
        );
        self.micro_job = Some(MicroJob { id, epoch, finished: false, cancel_noted: false });
    }

    fn poll_micro_job(&mut self) {
        let Some(id) = self.micro_job.as_ref().map(|job| job.id) else { return };
        let wanted = self.renderer.as_ref().map(|renderer| renderer.micro_wanted_epoch()).unwrap_or(0);
        let snap = self.jobs.snapshots().into_iter().find(|snap| snap.id == id);
        let Some(snap) = snap else { return };
        if snap.state.finished() {
            if self.micro_job.as_ref().is_some_and(|job| job.finished) {
                self.note_micro_job_stage(id);
                return;
            }
            if let Some(job) = self.micro_job.as_mut() {
                job.finished = true;
            }
            match snap.state {
                jarvig_core::JobState::Completed => match self.jobs.take_result::<MicroJobProduct>(id) {
                    Some(Ok(product)) => {
                        let epoch = product.epoch;
                        let accepted = self.renderer.as_mut().is_some_and(|renderer| {
                            renderer.accept_micro_build(product.epoch, product.key, product.mesh, product.queue_wait_us, product.skipped)
                        });
                        if accepted {
                            self.jobs.note(id, 0.85, "Uploading");
                        } else if let Some(renderer) = self.renderer.as_mut() {
                            renderer.abandon_micro_build(epoch);
                        }
                    }
                    Some(Err(error)) => {
                        if let Some(renderer) = self.renderer.as_mut() {
                            renderer.abandon_micro_build(self.micro_job.as_ref().map(|job| job.epoch).unwrap_or(0));
                        }
                        let noted = self.micro_job.as_ref().is_some_and(|job| job.cancel_noted);
                        if error != "cancelled" && !noted {
                            self.append(&format!("Einstein surface job failed: {error}. The published microgeometry stays."));
                        }
                    }
                    None => {}
                },
                jarvig_core::JobState::Cancelled => {
                    let epoch = self.micro_job.as_ref().map(|job| job.epoch).unwrap_or(0);
                    let _ = self.jobs.take_result::<MicroJobProduct>(id);
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.abandon_micro_build(epoch);
                    }
                }
                jarvig_core::JobState::Failed => {
                    let epoch = self.micro_job.as_ref().map(|job| job.epoch).unwrap_or(0);
                    let error = snap.error.unwrap_or_else(|| "unknown".into());
                    let _ = self.jobs.take_result::<MicroJobProduct>(id);
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.abandon_micro_build(epoch);
                    }
                    self.append(&format!("Einstein surface job failed: {error}. The published microgeometry stays."));
                }
                _ => {}
            }
            self.note_micro_job_stage(id);
            return;
        }
        let cancel = self.micro_job.as_ref().is_some_and(|job| !job.cancel_noted && job.epoch != wanted);
        if cancel {
            self.jobs.cancel(id);
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.note_micro_cancelled();
            }
            if let Some(job) = self.micro_job.as_mut() {
                job.cancel_noted = true;
            }
        }
    }

    fn note_micro_job_stage(&mut self, id: jarvig_core::JobId) {
        let stage = self.renderer.as_ref().map(|renderer| renderer.micro_stage()).unwrap_or("Ready");
        if stage == "Uploading" {
            self.jobs.note(id, 0.85, "Uploading");
        } else if stage == "Ready" {
            self.jobs.note(id, 1.0, "Ready");
        }
    }

    fn poll_jobs(&mut self) {
        self.poll_land_chunks();
        self.poll_registry_job();
        self.poll_micro_job();
        let snaps = self.jobs.snapshots();
        let micro_id = self.micro_job.as_ref().map(|job| job.id);
        let registry_id = self.registry_job;
        let land_id = self.land_chunk_job;
        for snap in &snaps {
            let tracked = self.parent_job.as_ref().map(|job| job.id) == Some(snap.id);
            if snap.state.finished() && !tracked && Some(snap.id) != micro_id && Some(snap.id) != registry_id && Some(snap.id) != land_id {
                let _ = self.jobs.take_result::<jarvig_core::ParentGeometry>(snap.id);
            }
        }
        let Some(tracked) = self.parent_job.as_ref().map(|job| job.id) else {
            if self.land_chunk_job.is_some() {
                self.refresh_status();
                return;
            }
            if self.job_line.is_empty() {
                return;
            }
            self.job_line.clear();
            self.refresh_status();
            return;
        };
        let Some(snap) = snaps.iter().find(|snap| snap.id == tracked) else { return };
        if self.parent_job.as_ref().is_some_and(|job| job.finished) {
            return;
        }
        let percent = (snap.progress * 100.0).round() as u32;
        let seconds = snap.elapsed_ms / 1000;
        let line = format!(
            "{} {} {:.0}% {} {:02}:{:02}",
            snap.name,
            snap.state.label(),
            snap.progress * 100.0,
            snap.stage,
            seconds / 60,
            seconds % 60
        );
        let state_changed = self.parent_job.as_ref().is_some_and(|job| job.last_state != snap.state);
        let bucket = percent / 5;
        let bucket_changed = self.parent_job.as_ref().is_some_and(|job| job.last_percent / 5 != bucket);
        if let Some(job) = self.parent_job.as_mut() {
            job.last_state = snap.state;
            job.last_percent = percent;
        }
        if snap.stage.contains("Loaded parent meshes") {
            self.rfc_cache_hit = true;
        }
        if snap.stage.contains("Simplifying") || snap.stage.contains("Writing the parent") {
            self.rfc_rebuild = true;
        }
        if line != self.job_line {
            self.job_line = line;
            self.refresh_status();
        }
        if state_changed || bucket_changed {
            if self.lod_capture {
                let line = format!("LOD_PROGRESS {} {:.0}% {}", snap.state.label(), snap.progress * 100.0, snap.stage);
                println!("{line}");
                let _ = std::fs::write(format!("{}/../target/rfc0001-progress.txt", env!("CARGO_MANIFEST_DIR")), &line);
            }
            self.append(&format!(
                "Job {} {} {:.0}% {} {:02}:{:02}",
                snap.id.0,
                snap.state.label(),
                snap.progress * 100.0,
                snap.stage,
                seconds / 60,
                seconds % 60
            ));
        }
        if !snap.state.finished() {
            return;
        }
        if let Some(job) = self.parent_job.as_mut() {
            job.finished = true;
        }
        match snap.state {
            jarvig_core::JobState::Completed => {
                let mesh = self.parent_job.as_ref().map(|job| job.mesh);
                let outcome = self.jobs.take_result::<jarvig_core::ParentGeometry>(tracked);
                match (mesh, outcome) {
                    (Some(mesh), Some(Ok(geometry))) => self.publish_parent_geometry(mesh, geometry, snap.elapsed_ms),
                    (Some(_), Some(Err(error))) => self.append(&format!("Parent mesh job failed: {error}. Leaf meshlets stay.")),
                    _ => self.append("Parent mesh job finished without a result. Leaf meshlets stay."),
                }
            }
            jarvig_core::JobState::Failed => {
                let _ = self.jobs.take_result::<jarvig_core::ParentGeometry>(tracked);
                self.append(&format!(
                    "Parent mesh job failed: {}. Leaf meshlets stay.",
                    snap.error.as_deref().unwrap_or("unknown")
                ));
            }
            jarvig_core::JobState::Cancelled => {
                let _ = self.jobs.take_result::<jarvig_core::ParentGeometry>(tracked);
                self.append("Parent mesh job cancelled. Leaf meshlets stay.");
            }
            _ => {}
        }
    }

    fn publish_parent_geometry(&mut self, mesh: jarvig_core::MeshId, geometry: jarvig_core::ParentGeometry, loaded_ms: u64) {
        let root_triangles = geometry.root_triangles;
        let parent_triangles = geometry.parent_triangles;
        let leaf_triangles = geometry.leaf_triangles;
        let elapsed_ms = geometry.build_ms;
        let empty_parents = geometry.empty_parents;
        self.rfc_levels = geometry.levels.clone();
        if geometry.vertices.is_empty() {
            self.append(&format!(
                "Parent mesh job finished in {elapsed_ms:.0} ms and did not reduce any node. The draw stays on the {leaf_triangles} leaf triangles."
            ));
            return;
        }
        if let Some(renderer) = self.renderer.as_mut() {
            if let Err(error) = renderer.set_parent_geometry(mesh, geometry) {
                self.append(&format!("Parent geometry did not upload: {}.", describe(error)));
                return;
            }
        }
        self.append(&format!(
            "RFC-0001 parent cache loaded in {loaded_ms} ms | original build time {elapsed_ms:.0} ms. {parent_triangles} parent triangles stored over {leaf_triangles} leaf triangles. The root cut is {root_triangles} triangles. Empty parents {empty_parents}."
        ));
    }

    fn record_async_frame(&mut self) {
        let stats = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats());
        let stage = self.micro_operation_stage();
        if matches!(stage.as_str(), "Queued" | "Sampling" | "Building" | "Uploading" | "Running") {
            self.async_busy = self.async_busy.saturating_add(1);
        }
        let row = AsyncFrame {
            base: stats.map(|stats| stats.hierarchy_lod_triangles).unwrap_or(0),
            leaf: stats.map(|stats| stats.hierarchy_leaf_triangles).unwrap_or(0),
            tris: stats.map(|stats| stats.micro_triangles).unwrap_or(0),
            verts: stats.map(|stats| stats.micro_vertices).unwrap_or(0),
            fingerprint: stats.map(|stats| stats.micro_fingerprint).unwrap_or(0),
            partial: stats.map(|stats| stats.micro_partial).unwrap_or(0),
            stage,
        };
        println!(
            "RFC0002_ASYNC_FRAME base={} micro={} verts={} fingerprint={:x} partial={} stage={} leaves={} frames={}",
            row.base, row.tris, row.verts, row.fingerprint, row.partial, row.stage, row.leaf, self.frames
        );
        self.async_frames.push(row);
    }

    fn fail_async(&mut self, reason: &str) {
        if self.async_done {
            return;
        }
        self.async_fault = reason.to_string();
        self.finish_async();
    }

    fn finish_async(&mut self) {
        if self.async_done {
            return;
        }
        self.async_done = true;
        let directory = format!("{}/../target/rfc0002-async", env!("CARGO_MANIFEST_DIR"));
        let _ = std::fs::create_dir_all(&directory);
        let mut reasons = Vec::new();
        if !self.async_fault.is_empty() {
            reasons.push(format!("\"{}\"", self.async_fault.replace('"', "'")));
        }
        if self.async_busy < 2 {
            reasons.push(format!("\"only {} presented frames occurred while generation was outstanding\"", self.async_busy));
        }
        let stats = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats());
        let cancelled = stats.map(|stats| stats.micro_cancelled).unwrap_or(0);
        let stale = stats.map(|stats| stats.micro_stale_discarded).unwrap_or(0);
        if cancelled.saturating_add(stale) == 0 {
            reasons.push("\"camera motion did not cancel or discard an obsolete build\"".into());
        }
        let mut saw_upload_hold = false;
        for pair in self.async_frames.windows(2) {
            let previous = &pair[0];
            let row = &pair[1];
            if row.partial != previous.partial || row.partial != 0 {
                reasons.push(format!("\"partial publication counter moved from {} to {}\"", previous.partial, row.partial));
                break;
            }
            if row.leaf != 6_122_214 {
                reasons.push(format!("\"leaf triangles became {}\"", row.leaf));
                break;
            }
            if row.base < 500_000 {
                reasons.push(format!("\"base surface fell to {} triangles\"", row.base));
                break;
            }
            if row.tris == 18432 && row.base != 6_122_214 {
                reasons.push(format!("\"close detail drew over {} base triangles\"", row.base));
                break;
            }
            let empty = row.tris == 0;
            if empty != (row.verts == 0) || empty != (row.fingerprint == 0) {
                reasons.push(format!("\"micro tris {} verts {} fingerprint {:x} is not a complete set\"", row.tris, row.verts, row.fingerprint));
                break;
            }
            if row.tris > 18432 {
                reasons.push(format!("\"micro tris {} exceeded the surface budget\"", row.tris));
                break;
            }
            let changed = row.tris != previous.tris || row.fingerprint != previous.fingerprint;
            if changed && !matches!(row.stage.as_str(), "Ready") {
                reasons.push(format!("\"microgeometry changed to {} during {}\"", row.tris, row.stage));
                break;
            }
            if matches!(previous.stage.as_str(), "Queued" | "Sampling" | "Building" | "Uploading") && row.tris == previous.tris && row.fingerprint == previous.fingerprint {
                saw_upload_hold = true;
            }
        }
        if !saw_upload_hold {
            reasons.push("\"the previous microgeometry was not held while a replacement was outstanding\"".into());
        }
        let close_rows: Vec<&AsyncFrame> = self.async_frames.iter().filter(|row| row.stage == "Ready" && row.tris == 18432).collect();
        if close_rows.len() < 2 {
            reasons.push(format!("\"settled close detail appeared {} times\"", close_rows.len()));
        } else if close_rows.iter().any(|row| row.fingerprint != close_rows[0].fingerprint) {
            reasons.push("\"returning to the close camera changed the fingerprint\"".into());
        }
        if !self.async_frames.iter().any(|row| row.stage == "Ready" && row.tris == 0 && row.base >= 500_000) {
            reasons.push("\"the far camera did not settle on zero detail over the base surface\"".into());
        }
        let passed = reasons.is_empty();
        let verdict = if passed { "PASS" } else { "FAIL" };
        let reasons_json = format!("[{}]", reasons.join(","));
        let fingerprint = close_rows.first().map(|row| row.fingerprint).unwrap_or(0);
        let json = format!(
            "{{\"verdict\":\"RFC0002_ASYNC_{verdict}\",\"frames\":{},\"busy_frames\":{},\"cancelled\":{cancelled},\"stale\":{stale},\"close_fingerprint\":\"{fingerprint:x}\",\"reasons\":{reasons_json}}}",
            self.async_frames.len(),
            self.async_busy
        );
        let _ = std::fs::write(format!("{directory}/report.json"), json);
        println!("RFC0002_ASYNC_{verdict} {directory}");
        println!("RFC0002_ASYNC_REASONS {reasons_json}");
        self.finish_lod_capture();
    }

    fn step_async_phases(&mut self) {
        match self.lod_phase {
            100 => {
                if self.lod_hold < 4 {
                    return;
                }
                self.micro_enabled = true;
                self.micro_surface = true;
                self.micro_color = false;
                self.push_micro_state();
                self.lod_phase = 101;
                self.lod_hold = 0;
            }
            101 => {
                if !self.micro_publish_settled() {
                    if self.lod_hold > 1200 {
                        self.fail_async("far detail did not settle");
                    }
                    return;
                }
                let tris = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats().micro_triangles).unwrap_or(1);
                if tris != 0 {
                    self.fail_async("far camera still showed micro triangles");
                    return;
                }
                self.record_async_frame();
                self.place_shop_camera(self.async_close);
                self.async_step = 0;
                self.lod_phase = 102;
                self.lod_hold = 0;
            }
            102 => {
                self.record_async_frame();
                self.async_step = self.async_step.saturating_add(1);
                if self.async_step >= 36 {
                    self.place_shop_camera(self.async_far);
                    self.lod_phase = 103;
                    self.lod_hold = 0;
                    return;
                }
                let distance = if self.async_step % 2 == 0 { self.async_close } else { self.async_far };
                self.place_shop_camera(distance);
            }
            103 => {
                self.record_async_frame();
                if !self.micro_publish_settled() {
                    if self.lod_hold > 1200 {
                        self.fail_async("return to zero detail did not settle");
                    }
                    return;
                }
                let tris = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats().micro_triangles).unwrap_or(1);
                if tris != 0 {
                    self.fail_async("settled far view kept micro triangles");
                    return;
                }
                self.place_shop_camera(self.async_close);
                self.lod_phase = 104;
                self.lod_hold = 0;
            }
            104 => {
                self.record_async_frame();
                if !self.micro_publish_settled() {
                    if self.lod_hold > 1200 {
                        self.fail_async("close detail did not publish");
                    }
                    return;
                }
                let tris = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats().micro_triangles).unwrap_or(0);
                if tris != 18432 {
                    self.fail_async(&format!("close detail published {tris} triangles"));
                    return;
                }
                self.place_shop_camera(self.async_far);
                self.lod_phase = 105;
                self.lod_hold = 0;
            }
            105 => {
                self.record_async_frame();
                if !self.micro_publish_settled() {
                    if self.lod_hold > 1200 {
                        self.fail_async("second far view did not settle");
                    }
                    return;
                }
                let tris = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats().micro_triangles).unwrap_or(1);
                if tris != 0 {
                    self.fail_async("second far view kept micro triangles");
                    return;
                }
                self.place_shop_camera(self.async_close);
                self.lod_phase = 106;
                self.lod_hold = 0;
            }
            106 => {
                self.record_async_frame();
                if !self.micro_publish_settled() {
                    if self.lod_hold > 1200 {
                        self.fail_async("second close view did not publish");
                    }
                    return;
                }
                self.finish_async();
            }
            _ => self.finish_async(),
        }
    }

    fn step_lod_capture(&mut self) {
        if !self.lod_capture {
            return;
        }
        self.lod_hold = self.lod_hold.saturating_add(1);
        if self.lod_phase >= 100 {
            self.step_async_phases();
            return;
        }
        if self.lod_phase >= 90 {
            self.step_refine_phases();
            return;
        }
        if self.lod_phase >= 70 {
            self.step_transition_phases();
            return;
        }
        if self.lod_phase >= 50 {
            self.step_micro_phases();
            return;
        }
        if self.lod_phase == 0 {
            if self.frames < 45 || self.find_named_entity("townshop").is_none() {
                return;
            }
            let Some(entity) = self.find_named_entity("townshop") else { return };
            if self.selection.replace(selection::SelectionItem::entity(entity).unwrap_or(selection::SelectionItem::Entity(entity))).is_err() {
                println!("LOD_FAIL townshop could not be selected");
                self.finish_lod_capture();
                return;
            }
            self.meshlet_shade = true;
            self.meshlet_debug = false;
            self.meshlet_frustum = false;
            self.meshlet_occlusion = false;
            self.cluster_hierarchy = true;
            self.hierarchy_error_px = 1.0;
            self.sync_view_menu();
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.set_meshlet_mode(false, true);
                renderer.set_meshlet_frustum(false);
                renderer.set_meshlet_occlusion(false);
                renderer.set_cluster_hierarchy(true);
                renderer.set_hierarchy_error_px(1.0);
            }
            self.sync_parent_geometry();
            self.lod_phase = 1;
            self.lod_hold = 0;
            println!("LOD_WAIT hierarchy armed");
            return;
        }
        if self.lod_phase == 1 {
            let stats = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats());
            let ready = stats.is_some_and(|stats| stats.hierarchy && stats.hierarchy_leaf_triangles > 0);
            let failed = self.job_line.contains("failed") || self.job_line.contains("did not upload") || self.job_line.contains("not written");
            if failed {
                println!("LOD_FAIL {}", self.job_line);
                self.finish_lod_capture();
                return;
            }
            if self.lod_hold > 90_000 {
                println!("LOD_FAIL timed out {}", self.job_line);
                self.finish_lod_capture();
                return;
            }
            if !ready {
                return;
            }
            if self.lod_hold < 20 {
                return;
            }
            if self.rfc0002_async {
                self.einstein_debug = false;
                self.micro_enabled = false;
                self.micro_color = false;
                self.micro_surface = true;
                self.micro_seed = 1;
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.set_einstein_debug(false, self.einstein_seed);
                    renderer.set_micro_surface(true);
                    renderer.set_microgeometry(false, self.micro_seed, false);
                }
                self.async_close = self.projected_error_distance(0.02, 3.20);
                self.async_far = self.projected_error_distance(0.02, 0.50);
                self.place_shop_camera(self.async_far);
                self.lod_phase = 100;
                self.lod_hold = 0;
                println!("RFC0002_ASYNC_PLAN close={:.2} far={:.2}", self.async_close, self.async_far);
                return;
            }
            if self.rfc0002_refine {
                self.einstein_debug = false;
                self.micro_enabled = false;
                self.micro_color = false;
                self.micro_surface = true;
                self.micro_seed = 1;
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.set_einstein_debug(false, self.einstein_seed);
                    renderer.set_micro_surface(true);
                    renderer.set_microgeometry(false, self.micro_seed, false);
                }
                self.transition_distances = self.transition_path();
                self.transition_forward = self.transition_distances.len().div_ceil(2);
                let near = self.shop_target().map(|target| target.half_z + 0.55).unwrap_or(1.2).clamp(1.05, 2.2);
                self.place_shop_camera(near);
                self.lod_phase = 90;
                self.lod_hold = 0;
                println!("RFC0002_REFINE_PLAN near={near:.2} dolly={}", self.transition_distances.len());
                return;
            }
            if self.rfc0002_transition {
                self.einstein_debug = false;
                self.micro_enabled = false;
                self.micro_color = false;
                self.micro_seed = 1;
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.set_einstein_debug(false, self.einstein_seed);
                    renderer.set_microgeometry(false, self.micro_seed, false);
                }
                self.transition_distances = self.transition_path();
                self.transition_forward = self.transition_distances.len().div_ceil(2);
                let near = self.shop_target().map(|target| target.half_z + 0.55).unwrap_or(1.2).clamp(1.05, 2.2);
                self.place_shop_camera(near);
                self.lod_phase = 70;
                self.lod_hold = 0;
                println!(
                    "RFC0002_TRANSITION_PLAN near={near:.2} samples={} forward={}",
                    self.transition_distances.len(),
                    self.transition_forward
                );
                return;
            }
            if self.rfc0002_micro {
                self.einstein_debug = false;
                self.micro_enabled = false;
                self.micro_color = false;
                self.micro_seed = 1;
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.set_einstein_debug(false, self.einstein_seed);
                    renderer.set_microgeometry(false, self.micro_seed, false);
                }
                let fit = self.shop_fit_distance();
                let switch_1px = self.projected_error_distance(0.02, 1.0);
                let close = fit.min(switch_1px * 0.55).clamp(2.4, 7.0);
                let partial = self.projected_error_distance(0.02, 1.0);
                let medium = switch_1px * 1.35;
                let far = switch_1px * 2.4;
                let near = self.shop_target().map(|target| target.half_z + 0.55).unwrap_or(1.2).clamp(1.05, 2.2);
                self.rfc_distances = vec![
                    ("close".into(), close),
                    ("partial".into(), partial),
                    ("medium".into(), medium),
                    ("far".into(), far),
                    ("near".into(), near),
                ];
                self.place_shop_camera(close);
                self.lod_phase = 50;
                self.lod_hold = 0;
                println!("RFC0002_MICRO_PLAN close={close:.2} partial={partial:.2} medium={medium:.2} far={far:.2} near={near:.2}");
                return;
            }
            if self.rfc0002 {
                self.einstein_debug = false;
                self.einstein_seed = 1;
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.set_einstein_debug(false, self.einstein_seed);
                }
                let fit = self.shop_fit_distance();
                let switch_1px = self.projected_error_distance(0.02, 1.0);
                let close = fit.min(switch_1px * 0.55).clamp(2.4, 7.0);
                let far = switch_1px * 2.4;
                self.rfc_distances = vec![("close".into(), close), ("far".into(), far)];
                self.place_shop_camera(close);
                self.lod_phase = 40;
                self.lod_hold = 0;
                println!("RFC0002_PLAN close={close:.2} far={far:.2}");
                return;
            }
            if self.rfc_runtime {
                self.rfc_usable_ms = self.boot_at.elapsed().as_millis();
                let fit = self.shop_fit_distance();
                let switch_1px = self.projected_error_distance(0.02, 1.0);
                let close = fit.min(switch_1px * 0.55).clamp(2.4, 7.0);
                let medium = switch_1px * 1.35;
                let far = switch_1px * 2.4;
                self.rfc_distances = rfc_runtime::dolly_distances(close, medium, far, 12);
                self.rfc_checkpoints = vec![("close".into(), close), ("medium".into(), medium), ("far".into(), far)];
                self.rfc_dolly_index = 0;
                self.rfc_checkpoint_index = 0;
                if let Some((_, distance)) = self.rfc_distances.first() {
                    self.place_shop_camera(*distance);
                }
                self.lod_phase = 20;
                self.lod_hold = 0;
                println!("RFC0001_RUNTIME dolly={} usable_ms={}", self.rfc_distances.len(), self.rfc_usable_ms);
                return;
            }
            if self.rfc0001 {
                self.rfc_plan = self.rfc_cameras();
                self.rfc_index = 0;
                let plan = self
                    .rfc_plan
                    .iter()
                    .take(3)
                    .map(|(_, name, distance)| format!("{name}={distance:.2}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("RFC0001_PLAN {plan} viewport={}x{}", self.viewport_px.0, self.viewport_px.1);
                self.begin_rfc_sample();
                return;
            }
            self.place_shop_camera(1.6);
            self.lod_phase = 2;
            self.lod_hold = 0;
            return;
        }
        if self.lod_phase == 40 {
            if self.lod_hold < 6 {
                return;
            }
            self.rfc0002_off = Some(self.capture_einstein_shot("off-close"));
            self.einstein_debug = true;
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.set_einstein_debug(true, self.einstein_seed);
            }
            self.lod_phase = 41;
            self.lod_hold = 0;
            return;
        }
        if self.lod_phase == 41 {
            if self.lod_hold < 6 {
                return;
            }
            self.rfc0002_on = Some(self.capture_einstein_shot("on-close"));
            self.lod_phase = 42;
            self.lod_hold = 0;
            return;
        }
        if self.lod_phase == 42 {
            if self.lod_hold < 4 {
                return;
            }
            self.rfc0002_again = Some(self.capture_einstein_shot("on-close-again"));
            self.place_shop_camera(self.rfc_distances.first().map(|(_, distance)| *distance).unwrap_or(2.4));
            if let Some(camera) = self.editor_camera.as_mut() {
                camera.yaw += 0.45;
                camera.reorbit();
            }
            self.lod_phase = 43;
            self.lod_hold = 0;
            return;
        }
        if self.lod_phase == 43 {
            if self.lod_hold < 4 {
                return;
            }
            self.rfc0002_orbit = Some(self.capture_einstein_shot("on-orbit"));
            let far = self.rfc_distances.get(1).map(|(_, distance)| *distance).unwrap_or(24.0);
            self.place_shop_camera(far);
            self.lod_phase = 44;
            self.lod_hold = 0;
            return;
        }
        if self.lod_phase == 44 {
            if self.lod_hold < 6 {
                return;
            }
            self.rfc0002_far = Some(self.capture_einstein_shot("on-far"));
            self.finish_rfc0002();
            return;
        }
        if self.lod_phase == 20 {
            if self.lod_hold < 1 {
                return;
            }
            self.record_runtime_frame();
            self.rfc_dolly_index = self.rfc_dolly_index.saturating_add(1);
            if self.rfc_dolly_index >= self.rfc_distances.len() {
                self.rfc_checkpoint_index = 0;
                self.arm_runtime_checkpoint(false);
                self.lod_phase = 21;
                self.lod_hold = 0;
                return;
            }
            if let Some((_, distance)) = self.rfc_distances.get(self.rfc_dolly_index) {
                self.place_shop_camera(*distance);
            }
            self.lod_hold = 0;
            return;
        }
        if self.lod_phase == 21 || self.lod_phase == 24 {
            if self.lod_hold < 4 {
                return;
            }
            self.capture_rfc_reference();
            self.hierarchy_error_px = 1.0;
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.set_hierarchy_error_px(1.0);
            }
            self.lod_phase = if self.lod_phase == 21 { 22 } else { 25 };
            self.lod_hold = 0;
            return;
        }
        if self.lod_phase == 22 || self.lod_phase == 25 {
            if self.lod_hold < 4 {
                return;
            }
            let integration = self.lod_phase == 25;
            self.record_runtime_checkpoint(integration);
            self.rfc_checkpoint_index = self.rfc_checkpoint_index.saturating_add(1);
            if self.rfc_checkpoint_index >= self.rfc_checkpoints.len() {
                if integration {
                    self.finish_rfc_runtime();
                    return;
                }
                self.meshlet_frustum = true;
                self.meshlet_occlusion = true;
                self.meshlet_debug = false;
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.set_meshlet_frustum(true);
                    renderer.set_meshlet_occlusion(true);
                    renderer.set_meshlet_mode(false, true);
                }
                self.rfc_checkpoint_index = 0;
                self.rfc_references.clear();
                self.arm_runtime_checkpoint(true);
                self.lod_phase = 24;
                self.lod_hold = 0;
                println!("RFC0001_RUNTIME integration frustum+occlusion");
                return;
            }
            self.arm_runtime_checkpoint(integration);
            self.lod_phase = if integration { 24 } else { 21 };
            self.lod_hold = 0;
            return;
        }
        if self.lod_phase == 11 {
            if self.lod_hold < 8 {
                return;
            }
            self.capture_rfc_reference();
            self.arm_rfc_sample();
            self.lod_phase = 10;
            self.lod_hold = 0;
            return;
        }
        if self.lod_phase == 10 {
            if self.lod_hold < 8 {
                return;
            }
            self.record_rfc_sample();
            self.rfc_index = self.rfc_index.saturating_add(1);
            if self.rfc_index >= self.rfc_plan.len() {
                self.finish_rfc0001();
                return;
            }
            self.begin_rfc_sample();
            return;
        }
        if self.lod_hold < 25 {
            return;
        }
        match self.lod_phase {
            2 => {
                self.save_lod_shot("close");
                self.place_shop_camera(8.0);
                self.lod_phase = 3;
                self.lod_hold = 0;
            }
            3 => {
                self.save_lod_shot("medium");
                self.place_shop_camera(28.0);
                self.lod_phase = 4;
                self.lod_hold = 0;
            }
            4 => {
                self.save_lod_shot("far");
                self.meshlet_debug = true;
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.set_meshlet_mode(true, true);
                }
                self.lod_phase = 5;
                self.lod_hold = 0;
            }
            5 => {
                self.save_lod_shot("far-colors");
                println!("LOD_DONE");
                self.finish_lod_capture();
            }
            _ => {}
        }
    }

    fn finish_lod_capture(&mut self) {
        unsafe {
            KillTimer(self.frame, TIMER_FRAME);
            PostQuitMessage(0);
        }
    }

    fn find_named_entity(&self, name: &str) -> Option<jarvig_core::EntityUuid> {
        let mut stack: Vec<jarvig_core::EntityUuid> = self.outliner.roots().to_vec();
        while let Some(id) = stack.pop() {
            if self.outliner.display_name(id) == Some(name) {
                return Some(id);
            }
            stack.extend(self.outliner.children(id).iter().copied());
        }
        None
    }

    fn place_shop_camera(&mut self, distance: f64) {
        let Some(target) = self.shop_target() else { return };
        let Some(camera) = self.editor_camera.as_mut() else { return };
        let mut flat_x = target.front.x;
        let mut flat_z = target.front.z;
        let length = (flat_x * flat_x + flat_z * flat_z).sqrt();
        if length < 1.0e-6 {
            flat_x = 0.0;
            flat_z = 1.0;
        } else {
            flat_x /= length;
            flat_z /= length;
        }
        let look_x = -flat_x;
        let look_z = -flat_z;
        camera.yaw = look_x.atan2(-look_z);
        camera.pitch = -0.10;
        camera.orbit_pivot = target.center;
        camera.orbit_distance = distance.max(1.0);
        camera.reorbit();
    }

    fn save_lod_shot(&self, name: &str) {
        let path = format!("{}/../target/lod-{name}.png", env!("CARGO_MANIFEST_DIR"));
        let status = format!("{}{}", self.job_line, self.gpu_scene_status());
        match capture_window_image(self.frame) {
            Ok(image) => {
                if let Err(error) = write_png(&path, &image) {
                    println!("LOD_SHOT {name} capture failed: {error}");
                } else {
                    println!("LOD_SHOT {name} {path}");
                }
            }
            Err(error) => println!("LOD_SHOT {name} capture failed: {error}"),
        }
        println!("LOD_STATUS {name} {status}");
        let _ = std::fs::write(format!("{}/../target/lod-{name}.txt", env!("CARGO_MANIFEST_DIR")), status);
    }

    fn rfc_cameras(&self) -> Vec<(f32, String, f64)> {
        let fit = self.shop_fit_distance();
        let switch_1px = self.projected_error_distance(0.02, 1.0);
        let close = fit.min(switch_1px * 0.55).clamp(2.4, 7.0);
        let medium = switch_1px * 1.35;
        let far = switch_1px * 2.4;
        let cameras = [("close", close), ("medium", medium), ("far", far)];
        let mut plan = Vec::new();
        for threshold in [0.5_f32, 1.0, 2.0, 4.0] {
            for (name, distance) in cameras {
                plan.push((threshold, name.to_string(), distance));
            }
        }
        plan
    }

    fn projected_error_distance(&self, error_m: f64, threshold_px: f64) -> f64 {
        let fov = self.editor_camera.as_ref().map(|camera| camera.vertical_fov_radians).unwrap_or(42.0_f64.to_radians());
        let tan_y = (fov * 0.5).tan().max(0.05);
        let height = self.viewport_px.1.max(1) as f64;
        (error_m * (height * 0.5) / (threshold_px.max(0.05) * tan_y)).max(1.0)
    }

    fn shop_fit_distance(&self) -> f64 {
        let Some(target) = self.shop_target() else { return 4.0 };
        let fov = self.editor_camera.as_ref().map(|camera| camera.vertical_fov_radians).unwrap_or(42.0_f64.to_radians());
        let half_y = (fov * 0.5).tan().max(0.05);
        let aspect = {
            let (width, height) = self.viewport_px;
            if height == 0 { 1.6 } else { width as f64 / height as f64 }
        };
        let half_x = half_y * aspect.max(0.5);
        let vertical = target.half_y / half_y;
        let horizontal = target.half_x.max(target.half_z) / half_x;
        (vertical.max(horizontal) * 1.55).max(2.4)
    }

    fn shop_target(&self) -> Option<ShopTarget> {
        let entity = self.find_named_entity("townshop")?;
        let snapshot = self.engine.world().extract(jarvig_core::RenderFrameId(77)).ok()?;
        let instance = snapshot.instances().iter().find(|instance| instance.entity == entity)?;
        let min = instance.bounds.aabb.min;
        let max = instance.bounds.aabb.max;
        let scale = instance.scale;
        let rotation = instance.pose.rotation;
        let origin = instance.pose.translation;
        let mut corners = [Vec3::ZERO; 8];
        let mut index = 0;
        for x in [min[0], max[0]] {
            for y in [min[1], max[1]] {
                for z in [min[2], max[2]] {
                    let local = Vec3::new(x as f64 * scale.x, y as f64 * scale.y, z as f64 * scale.z);
                    corners[index] = origin + rotation.rotate(local);
                    index += 1;
                }
            }
        }
        let center = corners.iter().fold(Vec3::ZERO, |sum, corner| sum + *corner).scale(1.0 / 8.0);
        let front = rotation.rotate(Vec3::new(0.0, 0.0, -1.0));
        let half = |axis: usize| {
            let span = (max[axis] - min[axis]) as f64;
            let scaled = match axis {
                0 => span * scale.x.abs(),
                1 => span * scale.y.abs(),
                _ => span * scale.z.abs(),
            };
            scaled.abs() * 0.5
        };
        Some(ShopTarget { center, front, corners, half_x: half(0), half_y: half(1), half_z: half(2) })
    }

    fn begin_rfc_sample(&mut self) {
        let Some((_, camera, _)) = self.rfc_plan.get(self.rfc_index).cloned() else { return };
        self.arm_rfc_sample();
        if self.rfc_references.iter().any(|reference| reference.camera == camera) {
            self.lod_phase = 10;
        } else {
            self.hierarchy_error_px = 0.25;
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.set_hierarchy_error_px(0.25);
            }
            self.lod_phase = 11;
        }
        self.lod_hold = 0;
    }

    fn arm_rfc_sample(&mut self) {
        let Some((threshold, _, distance)) = self.rfc_plan.get(self.rfc_index).cloned() else { return };
        self.hierarchy_error_px = threshold;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_hierarchy_error_px(threshold);
            renderer.set_meshlet_mode(false, true);
        }
        self.meshlet_debug = false;
        self.place_shop_camera(distance);
    }

    fn capture_rfc_reference(&mut self) {
        let Some((_, camera, _)) = self.rfc_plan.get(self.rfc_index).cloned() else { return };
        let stats = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats());
        let parents = stats.map(|stats| stats.hierarchy_parents).unwrap_or(1);
        let triangles = stats.map(|stats| stats.hierarchy_lod_triangles).unwrap_or(0);
        let original = stats.map(|stats| stats.hierarchy_leaf_triangles).unwrap_or(0);
        if parents > 0 || original == 0 || (!self.meshlet_frustum && triangles + 64 < original) {
            self.rfc_ref_dirty = true;
            println!("RFC0001_REF {camera} dirty parents={parents} tris={triangles}/{original}");
        } else {
            println!("RFC0001_REF {camera} leaves tris={triangles}/{original}");
        }
        let directory = self.rfc_directory();
        let _ = std::fs::create_dir_all(&directory);
        match capture_window_image(self.frame) {
            Ok(image) => {
                let path = format!("{directory}/ref-{camera}.png");
                if let Err(error) = write_png(&path, &image) {
                    self.rfc_ref_dirty = true;
                    println!("RFC0001_REF {camera} png failed: {error}");
                }
                let (origin_x, origin_y, view_w, view_h) = viewport_origin(self.frame, self.panel_hwnd(PERSPECTIVE));
                self.rfc_references.push(RfcReference {
                    camera,
                    rgba: image.rgba,
                    width: image.width,
                    height: image.height,
                    origin_x,
                    origin_y,
                    view_w,
                    view_h,
                });
            }
            Err(error) => {
                self.rfc_ref_dirty = true;
                println!("RFC0001_REF {camera} capture failed: {error}");
            }
        }
    }

    fn record_rfc_sample(&mut self) {
        let Some((threshold, camera, distance)) = self.rfc_plan.get(self.rfc_index).cloned() else { return };
        let stats = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats());
        let (depth, leaves, parents, triangles, original, select_us, empty_parents, root_triangles) = stats
            .map(|stats| {
                (
                    stats.hierarchy_depth,
                    stats.hierarchy_leaves,
                    stats.hierarchy_parents,
                    stats.hierarchy_lod_triangles,
                    stats.hierarchy_leaf_triangles,
                    stats.hierarchy_select_us,
                    stats.hierarchy_empty_parents,
                    stats.hierarchy_root_triangles,
                )
            })
            .unwrap_or((0, 0, 0, 0, 0, 0, 0, 0));
        let reduction = if original == 0 { 0.0 } else { (triangles as f32 / original as f32 - 1.0) * 100.0 };
        let name = format!("{threshold:.1}-{camera}");
        let directory = self.rfc_directory();
        let _ = std::fs::create_dir_all(&directory);
        let mut hole_ratio = 1.0;
        let projection = self.shop_projection();
        let in_frame = projection.as_ref().is_some_and(|projection| projection.in_frame);
        let shop_height_px = projection.as_ref().map(|projection| projection.height as f32).unwrap_or(0.0);
        let viewport_h = self.viewport_px.1;
        if let Ok(image) = capture_window_image(self.frame) {
            let path = format!("{directory}/{name}.png");
            if let Err(error) = write_png(&path, &image) {
                println!("RFC0001_PNG {name} failed: {error}");
            } else {
                println!("RFC0001_PNG {name} {path}");
            }
            if let Some(reference) = self.rfc_references.iter().find(|reference| reference.camera == camera) {
                if reference.width == image.width && reference.height == image.height {
                    let sky = sky_color(&reference.rgba, reference.width, reference.origin_x, reference.origin_y, reference.view_w, reference.view_h);
                    let region = shop_region(projection.as_ref(), reference.origin_x, reference.origin_y, reference.view_w, reference.view_h, image.width, image.height);
                    let slack = (threshold.round() as i32).clamp(2, 4);
                    let (holes, solid) = count_silhouette_holes(&reference.rgba, &image.rgba, image.width, region, sky, slack);
                    hole_ratio = if solid < 64 { 1.0 } else { holes as f32 / solid as f32 };
                    println!("RFC0001_HOLE {name} holes={holes} solid={solid} ratio={hole_ratio:.4} in_frame={in_frame} shop_h={shop_height_px:.0}");
                }
            }
        }
        self.rfc_rows.push(RfcSample {
            threshold,
            camera,
            distance,
            depth,
            leaves,
            parents,
            triangles,
            original,
            reduction,
            select_us,
            empty_parents,
            root_triangles,
            hole_ratio,
            in_frame,
            shop_height_px,
            viewport_h,
        });
        println!("RFC0001_ROW threshold={threshold:.1} camera={name} depth={depth} leaves={leaves} parents={parents} tris={triangles}/{original} reduction={reduction:.1} select_us={select_us} empty={empty_parents} root={root_triangles} hole={hole_ratio:.4} in_frame={in_frame}");
    }

    fn rfc_directory(&self) -> String {
        let name = if self.rfc_runtime { "rfc0001-runtime" } else { "rfc0001" };
        format!("{}/../target/{name}", env!("CARGO_MANIFEST_DIR"))
    }

    fn arm_runtime_checkpoint(&mut self, _integration: bool) {
        let Some((name, distance)) = self.rfc_checkpoints.get(self.rfc_checkpoint_index).cloned() else { return };
        self.rfc_plan = vec![(1.0, name, distance)];
        self.rfc_index = 0;
        self.hierarchy_error_px = 0.25;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_hierarchy_error_px(0.25);
            renderer.set_meshlet_mode(false, true);
        }
        self.meshlet_debug = false;
        self.place_shop_camera(distance);
    }

    fn record_runtime_frame(&mut self) {
        let Some((leg, distance)) = self.rfc_distances.get(self.rfc_dolly_index).cloned() else { return };
        let stats = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats());
        let frame = stats
            .map(|stats| rfc_runtime::RuntimeFrame {
                leg: leg.clone(),
                distance_m: distance,
                depth: stats.hierarchy_depth,
                leaves: stats.hierarchy_leaves,
                parents: stats.hierarchy_parents,
                triangles: stats.hierarchy_lod_triangles,
                original: stats.hierarchy_leaf_triangles,
                covered: stats.hierarchy_covered_leaves,
                select_us: stats.hierarchy_select_us,
                cut_us: stats.hierarchy_cut_us,
                upload_us: stats.hierarchy_upload_us,
                upload_bytes: stats.hierarchy_upload_bytes,
                draw_prepare_us: stats.hierarchy_draw_prepare_us,
                renderer_cpu_us: stats.renderer_cpu_us,
                clusters: stats.hierarchy_leaves.saturating_add(stats.hierarchy_parents),
            })
            .unwrap_or(rfc_runtime::RuntimeFrame {
                leg,
                distance_m: distance,
                depth: 0,
                leaves: 0,
                parents: 0,
                triangles: 0,
                original: 0,
                covered: 0,
                select_us: 0,
                cut_us: 0,
                upload_us: 0,
                upload_bytes: 0,
                draw_prepare_us: 0,
                renderer_cpu_us: 0,
                clusters: 0,
            });
        if self.rfc_dolly_index % 12 == 0 {
            if let Ok(image) = capture_window_image(self.frame) {
                let directory = self.rfc_directory();
                let _ = std::fs::create_dir_all(&directory);
                let _ = write_png(&format!("{directory}/dolly-{:02}-{:.0}m.png", self.rfc_dolly_index, distance), &image);
            }
        }
        println!(
            "RFC0001_DOLLY {} {:.2}m leaves={} parents={} tris={} covered={} select_us={} cut_us={} upload_us={} upload_b={} prepare_us={} cpu_us={}",
            frame.leg, frame.distance_m, frame.leaves, frame.parents, frame.triangles, frame.covered, frame.select_us, frame.cut_us, frame.upload_us, frame.upload_bytes, frame.draw_prepare_us, frame.renderer_cpu_us
        );
        self.rfc_dolly.push(frame);
    }

    fn record_runtime_checkpoint(&mut self, integration: bool) {
        let Some((name, _)) = self.rfc_checkpoints.get(self.rfc_checkpoint_index).cloned() else { return };
        let stats = self.renderer.as_ref().map(|renderer| renderer.gpu_scene_stats());
        let mut hole_ratio = 1.0;
        let directory = self.rfc_directory();
        let _ = std::fs::create_dir_all(&directory);
        let label = if integration { format!("integrate-{name}") } else { format!("check-{name}") };
        if let Ok(image) = capture_window_image(self.frame) {
            let _ = write_png(&format!("{directory}/{label}.png"), &image);
            if let Some(reference) = self.rfc_references.iter().find(|reference| reference.camera == name) {
                if reference.width == image.width && reference.height == image.height {
                    let sky = sky_color(&reference.rgba, reference.width, reference.origin_x, reference.origin_y, reference.view_w, reference.view_h);
                    let projection = self.shop_projection();
                    let region = shop_region(projection.as_ref(), reference.origin_x, reference.origin_y, reference.view_w, reference.view_h, image.width, image.height);
                    let (holes, solid) = count_silhouette_holes(&reference.rgba, &image.rgba, image.width, region, sky, 3);
                    hole_ratio = if solid < 64 { 1.0 } else { holes as f32 / solid as f32 };
                }
            }
        }
        let (leaves, parents, triangles) = stats.map(|stats| (stats.hierarchy_leaves, stats.hierarchy_parents, stats.hierarchy_lod_triangles)).unwrap_or((0, 0, 0));
        println!("RFC0001_CHECK {label} leaves={leaves} parents={parents} tris={triangles} hole={hole_ratio:.4}");
        if integration {
            let sample = stats
                .map(|stats| rfc_runtime::IntegrateSample {
                    camera: name.clone(),
                    hierarchy: stats.hierarchy,
                    frustum: stats.frustum,
                    occlusion: stats.occlusion,
                    colors_off: !self.meshlet_debug,
                    meshlets: stats.meshlets,
                    submitted: stats.meshlets_submitted,
                    frustum_rejected: stats.meshlets_rejected,
                    occlusion_rejected: stats.occlusion_rejected,
                    conservative_visible: stats.conservative_visible,
                    leaves,
                    parents,
                    triangles,
                    select_us: stats.hierarchy_select_us,
                    hole_ratio,
                })
                .unwrap_or(rfc_runtime::IntegrateSample {
                    camera: name,
                    hierarchy: false,
                    frustum: false,
                    occlusion: false,
                    colors_off: false,
                    meshlets: 0,
                    submitted: 0,
                    frustum_rejected: 0,
                    occlusion_rejected: 0,
                    conservative_visible: 0,
                    leaves: 0,
                    parents: 0,
                    triangles: 0,
                    select_us: 0,
                    hole_ratio,
                });
            self.rfc_integrate.push(sample);
        } else {
            self.rfc_holes.push(rfc_runtime::HoleSample { camera: name, hole_ratio, triangles, leaves, parents });
        }
    }

    fn finish_rfc_runtime(&mut self) {
        let directory = self.rfc_directory();
        let _ = std::fs::create_dir_all(&directory);
        let leaf_count = self.rfc_dolly.iter().map(|frame| frame.covered).max().unwrap_or(0);
        let mut reasons = rfc_runtime::judge_dolly(&self.rfc_dolly, leaf_count);
        if !self.rfc_cache_hit || self.rfc_rebuild {
            reasons.push("\"cold start rebuilt the sidecar\"".into());
        }
        if self.rfc_usable_ms == 0 {
            reasons.push("\"hierarchy usable time was not recorded\"".into());
        }
        for hole in &self.rfc_holes {
            if hole.hole_ratio > 0.015 {
                reasons.push(format!("\"dolly {} silhouette holes {:.2}%\"", hole.camera, hole.hole_ratio * 100.0));
            }
        }
        reasons.extend(rfc_runtime::judge_integration(&self.rfc_integrate));
        let residency = self.renderer.as_ref().map(|renderer| renderer.hierarchy_residency()).unwrap_or_default();
        let sidecar = self.sidecar_bytes();
        let meshlet_file = self.meshlet_sidecar_bytes();
        let mut csv = String::from("leg,distance_m,depth,leaves,parents,triangles,original,covered,select_us,cut_us,upload_us,upload_bytes,draw_prepare_us,renderer_cpu_us,clusters\n");
        for frame in &self.rfc_dolly {
            csv.push_str(&format!(
                "{},{:.2},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                frame.leg, frame.distance_m, frame.depth, frame.leaves, frame.parents, frame.triangles, frame.original, frame.covered, frame.select_us, frame.cut_us, frame.upload_us, frame.upload_bytes, frame.draw_prepare_us, frame.renderer_cpu_us, frame.clusters
            ));
        }
        let mut selects: Vec<u32> = self.rfc_dolly.iter().map(|frame| frame.select_us).collect();
        selects.sort_unstable();
        let median = selects.get(selects.len() / 2).copied().unwrap_or(0);
        let max = selects.iter().copied().max().unwrap_or(0);
        let mut holes_json = String::from("[");
        for (index, hole) in self.rfc_holes.iter().enumerate() {
            if index > 0 {
                holes_json.push(',');
            }
            holes_json.push_str(&format!(
                "{{\"camera\":\"{}\",\"hole_ratio\":{:.4},\"triangles\":{},\"leaves\":{},\"parents\":{}}}",
                hole.camera, hole.hole_ratio, hole.triangles, hole.leaves, hole.parents
            ));
        }
        holes_json.push(']');
        let mut integrate_json = String::from("[");
        for (index, sample) in self.rfc_integrate.iter().enumerate() {
            if index > 0 {
                integrate_json.push(',');
            }
            integrate_json.push_str(&format!(
                "{{\"camera\":\"{}\",\"hierarchy\":{},\"frustum\":{},\"occlusion\":{},\"colors_off\":{},\"meshlets\":{},\"submitted\":{},\"frustum_rejected\":{},\"occlusion_rejected\":{},\"conservative_visible\":{},\"leaves\":{},\"parents\":{},\"triangles\":{},\"select_us\":{},\"hole_ratio\":{:.4}}}",
                sample.camera, sample.hierarchy, sample.frustum, sample.occlusion, sample.colors_off, sample.meshlets, sample.submitted, sample.frustum_rejected, sample.occlusion_rejected, sample.conservative_visible, sample.leaves, sample.parents, sample.triangles, sample.select_us, sample.hole_ratio
            ));
        }
        integrate_json.push(']');
        let passed = reasons.is_empty();
        let verdict = if passed { "PASS" } else { "FAIL" };
        let reasons_json = format!("[{}]", reasons.join(","));
        let json = format!(
            "{{\"verdict\":\"RFC0001_RUNTIME_{verdict}\",\"cache_hit\":{},\"rebuild\":{},\"usable_ms\":{},\"recorded_build_ms\":{:.1},\"select_median_us\":{median},\"select_max_us\":{max},\"gpu_frame_measured\":false,\"memory\":{{\"leaf_meshlet_bytes\":{},\"meshlet_sidecar_bytes\":{meshlet_file},\"hierarchy_cpu_bytes\":{},\"parent_gpu_static_bytes\":{},\"parent_gpu_frame_bytes\":{},\"sidecar_bytes\":{sidecar},\"peak_build_bytes\":null}},\"holes\":{holes_json},\"integration\":{integrate_json},\"reasons\":{reasons_json}}}",
            self.rfc_cache_hit, self.rfc_rebuild, self.rfc_usable_ms, residency.recorded_build_ms, residency.leaf_meshlet_bytes, residency.hierarchy_cpu_bytes, residency.parent_gpu_static_bytes, residency.parent_gpu_frame_bytes
        );
        let _ = std::fs::write(format!("{directory}/frames.csv"), csv);
        let _ = std::fs::write(format!("{directory}/report.json"), json);
        println!("RFC0001_RUNTIME_{verdict} {directory}");
        println!("RFC0001_RUNTIME_REASONS {reasons_json}");
        self.finish_lod_capture();
    }

    fn sidecar_bytes(&self) -> u64 {
        self.asset_sidecar_bytes("jarvigparents")
    }

    fn meshlet_sidecar_bytes(&self) -> u64 {
        self.asset_sidecar_bytes("jarvigmeshlets")
    }

    fn asset_sidecar_bytes(&self, extension: &str) -> u64 {
        let Some(project_file) = &self.project_file else { return 0 };
        let Ok(project) = jarvig_core::load_project_file(project_file) else { return 0 };
        let Some(root) = project_file.parent() else { return 0 };
        let path = root.join(project.intermediate_directory).join("Meshes").join(format!("affe7d8d-e3e1-448a-a008-76c6e6266867.{extension}"));
        std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
    }

    fn shop_projection(&self) -> Option<ShopProjection> {
        let target = self.shop_target()?;
        let camera = self.editor_camera.as_ref()?;
        let (view_w, view_h) = self.viewport_px;
        if view_w == 0 || view_h == 0 {
            return None;
        }
        project_shop(camera.position, camera.forward(), camera.right(), camera.camera_up(), camera.vertical_fov_radians, view_w, view_h, &target.corners)
    }

    fn finish_rfc0001(&mut self) {
        let directory = self.rfc_directory();
        let _ = std::fs::create_dir_all(&directory);
        let mut csv = String::from("threshold_px,camera,distance_m,depth,leaves,parents,triangles,original,reduction_pct,select_us,empty_parents,root_triangles,hole_ratio,in_frame,shop_height_px\n");
        let mut json_rows = String::from("[");
        for (index, row) in self.rfc_rows.iter().enumerate() {
            if index > 0 {
                json_rows.push(',');
            }
            csv.push_str(&format!(
                "{:.1},{},{:.2},{},{},{},{},{},{:.2},{},{},{},{:.4},{},{:.1}\n",
                row.threshold, row.camera, row.distance, row.depth, row.leaves, row.parents, row.triangles, row.original, row.reduction, row.select_us, row.empty_parents, row.root_triangles, row.hole_ratio, row.in_frame, row.shop_height_px
            ));
            json_rows.push_str(&format!(
                "{{\"threshold_px\":{:.1},\"camera\":\"{}\",\"distance_m\":{:.2},\"depth\":{},\"leaves\":{},\"parents\":{},\"triangles\":{},\"original\":{},\"reduction_pct\":{:.2},\"select_us\":{},\"empty_parents\":{},\"root_triangles\":{},\"hole_ratio\":{:.4},\"in_frame\":{},\"shop_height_px\":{:.1}}}",
                row.threshold, row.camera, row.distance, row.depth, row.leaves, row.parents, row.triangles, row.original, row.reduction, row.select_us, row.empty_parents, row.root_triangles, row.hole_ratio, row.in_frame, row.shop_height_px
            ));
        }
        json_rows.push(']');
        let mut level_json = String::from("[");
        let mut uncovered = 0u32;
        for (index, level) in self.rfc_levels.iter().enumerate() {
            if index > 0 {
                level_json.push(',');
            }
            let missed = level.nodes.saturating_sub(level.bounds_covered).saturating_sub(level.empty_parents);
            uncovered = uncovered.saturating_add(missed);
            level_json.push_str(&format!(
                "{{\"level\":{},\"nodes\":{},\"parent_triangles\":{},\"child_triangles\":{},\"bounds_covered\":{},\"empty_parents\":{}}}",
                level.level, level.nodes, level.parent_triangles, level.child_triangles, level.bounds_covered, level.empty_parents
            ));
        }
        level_json.push(']');
        let parent_nodes = self.rfc_levels.iter().fold(0u32, |sum, level| sum.saturating_add(level.nodes));
        let (passed, reasons) = judge_rfc0001(&self.rfc_rows, self.rfc_cache_hit, self.rfc_ref_dirty, uncovered, parent_nodes);
        let verdict = if passed { "PASS" } else { "FAIL" };
        let cache = if self.rfc_cache_hit { "true" } else { "false" };
        let json = format!("{{\"verdict\":\"{verdict}\",\"cache_hit\":{cache},\"uncovered_parents\":{uncovered},\"reasons\":{reasons},\"levels\":{level_json},\"rows\":{json_rows}}}");
        let _ = std::fs::write(format!("{directory}/report.csv"), csv);
        let _ = std::fs::write(format!("{directory}/report.json"), json);
        println!("RFC0001_{verdict} {directory}");
        println!("RFC0001_REASONS {reasons}");
        self.finish_lod_capture();
    }

    fn set_hierarchy_error(&mut self, pixels: f32) {
        self.hierarchy_error_px = pixels;
        self.sync_view_menu();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_hierarchy_error_px(pixels);
        }
        self.append(&format!("Hierarchy error threshold is {pixels:.1} px. A parent replaces its leaves only when its projected error is under that. 0.5 stays finer. 4 simplifies sooner."));
        self.refresh_status();
    }

    fn parent_cache_path(&self, asset: jarvig_core::AssetId) -> Option<std::path::PathBuf> {
        let project_file = self.project_file.clone()?;
        let project = jarvig_core::load_project_file(&project_file).ok()?;
        let root = project_file.parent()?;
        Some(root.join(project.intermediate_directory).join("Meshes").join(format!("{}.jarvigparents", asset.0)))
    }

    fn show_background_jobs(&mut self) {
        let snaps = self.jobs.snapshots();
        if snaps.is_empty() {
            self.append("Background jobs: none.");
            return;
        }
        self.append("Background jobs:");
        for snap in snaps {
            let seconds = snap.elapsed_ms / 1000;
            self.append(&format!(
                "  {} {} {} {:.0}% {} {:02}:{:02}{}",
                snap.id.0,
                snap.name,
                snap.state.label(),
                snap.progress * 100.0,
                snap.stage,
                seconds / 60,
                seconds % 60,
                snap.error.as_ref().map(|error| format!(" {error}")).unwrap_or_default()
            ));
        }
    }

    fn cancel_background_job(&mut self) {
        if let Some(job) = self.micro_job.as_ref() {
            if !job.finished {
                self.jobs.cancel(job.id);
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.note_micro_cancelled();
                }
                if let Some(job) = self.micro_job.as_mut() {
                    job.cancel_noted = true;
                }
                self.append("Cancel requested for the Einstein surface job. The published microgeometry stays until a newer build is resident.");
                return;
            }
        }
        let Some(job) = self.parent_job.as_ref() else {
            self.append("No background job is queued.");
            return;
        };
        if job.finished {
            self.append("The parent-mesh job has already finished.");
            return;
        }
        self.jobs.cancel(job.id);
        self.append("Cancel requested for the parent-mesh job. Leaf meshlets stay until you turn Cluster Hierarchy on again.");
    }

    fn toggle_meshlet_highlight(&mut self) {
        self.meshlet_highlight = !self.meshlet_highlight;
        self.sync_view_menu();
        if !self.meshlet_highlight {
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.set_meshlet_highlight(None);
            }
            self.append("Cluster highlight is off.");
        } else {
            self.append("Cluster highlight is on. Click the roof. The output log names that cluster, and Show Meshlet Colors paints it magenta.");
        }
        self.refresh_status();
    }

    fn highlight_meshlet_under_cursor(&mut self, ray: jarvig_core::PickRay, entity: jarvig_core::EntityId) {
        let records = self.engine.meshlet_records(entity);
        if records.is_empty() {
            self.append("This actor has no meshlets.");
            return;
        }
        let Ok(snapshot) = self.engine.world().extract(jarvig_core::RenderFrameId(31)) else { return };
        let Some(instance) = snapshot.instances().iter().find(|instance| instance.entity == entity) else { return };
        let offset = jarvig_core::Vec3::new(
            ray.origin.x - instance.pose.translation.x,
            ray.origin.y - instance.pose.translation.y,
            ray.origin.z - instance.pose.translation.z,
        );
        let local_origin = instance.pose.rotation.conjugate().rotate(offset);
        let local_direction = instance.pose.rotation.conjugate().rotate(ray.direction);
        let scale = instance.scale;
        let origin = [(local_origin.x / scale.x) as f32, (local_origin.y / scale.y) as f32, (local_origin.z / scale.z) as f32];
        let direction = [(local_direction.x / scale.x) as f32, (local_direction.y / scale.y) as f32, (local_direction.z / scale.z) as f32];
        let Some(cluster) = jarvig_core::closest_meshlet_on_ray(&records, origin, direction) else {
            self.append("The cursor ray missed every meshlet sphere.");
            return;
        };
        let record = &records[cluster as usize];
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_meshlet_highlight(Some(cluster));
        }
        self.append(&format!(
            "Cluster {cluster} center {:.3} {:.3} {:.3} radius {:.3} triangles {}",
            record.center[0], record.center[1], record.center[2], record.radius, record.triangles
        ));
    }

    fn toggle_meshlet_occlusion(&mut self) {
        self.meshlet_occlusion = !self.meshlet_occlusion;
        self.sync_view_menu();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_meshlet_occlusion(self.meshlet_occlusion);
        }
        if self.meshlet_occlusion {
            self.append("Occlusion cull is on. occ is confidently hidden. cons is inside the frustum and kept because the test was unsure. Green and amber are drawn. Blue is hidden. The original triangle list is unchanged.");
        } else {
            self.append("Occlusion cull is off. Frustum results are unchanged. No cluster is removed for being behind another.");
        }
        self.refresh_status();
    }

    fn toggle_meshlet_frustum(&mut self) {
        self.meshlet_frustum = !self.meshlet_frustum;
        self.sync_view_menu();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.set_meshlet_frustum(self.meshlet_frustum);
        }
        if self.meshlet_frustum {
            self.append("Frustum cull is on. Green meshlets are inside the camera. Red meshlets are outside it. Draw From Meshlets submits only the green ones. The original triangle list is unchanged.");
        } else {
            self.append("Frustum cull is off. The meshlet path submits all  stored clusters again.");
        }
        self.refresh_status();
    }

    fn toggle_meshlet_debug(&mut self, shade: bool) {
        if shade {
            self.meshlet_shade = !self.meshlet_shade;
        } else {
            self.meshlet_debug = !self.meshlet_debug;
        }
        self.sync_view_menu();
        self.log_meshlet_state();
        let started = std::time::Instant::now();
        let uploaded = self.sync_meshlet_debug();
        if self.meshlet_debug || self.meshlet_shade {
            let elapsed_ms = started.elapsed().as_secs_f32() * 1000.0;
            if uploaded {
                self.append(&format!("JRV-0028 debug upload {elapsed_ms:.1} ms from the stored clusters. The sidecar was not rebuilt. The next toggle of this actor reuses the upload."));
            } else {
                self.append(&format!("JRV-0028 debug toggle reused the upload ({elapsed_ms:.2} ms)."));
            }
        }
        self.refresh_status();
    }

    fn log_meshlet_state(&mut self) {
        if !self.meshlet_debug && !self.meshlet_shade {
            self.append("Meshlet view is off. The shop is the original triangle list.");
            return;
        }
        let Some(entity) = self.selection.primary_entity() else {
            self.append("Select the imported shop, then use View > Show Meshlet Colors or View > Draw From Meshlets.");
            return;
        };
        let has_meshlets = self.engine.world().authored_mesh(entity).and_then(|(_, _, _, _, _, _, mesh, _)| match mesh {
            jarvig_core::MeshAssetRef::Asset { id, .. } => self.engine.mesh_asset_library().meshlets(id).map(|_| ()),
            _ => None,
        });
        if has_meshlets.is_none() {
            self.append("This actor has no stored meshlets. The view stays on the original triangles.");
            return;
        }
        let sentence = match (self.meshlet_debug, self.meshlet_shade) {
            (true, true) => "Meshlet colors are on. Green is visible. Amber is kept because the occlusion test was unsure. Red is outside the frustum. Blue is occluded. Draw From Meshlets submits green and amber.",
            (true, false) => "Meshlet colors are on. Green is visible. Amber is kept because the occlusion test was unsure. Red is outside the frustum. Blue is occluded. The original triangle list is still the shaded draw.",
            (false, true) => "The shop is drawn from meshlets, with its normal materials. Occluded clusters are not submitted. Turn this off to see the original triangle list.",
            (false, false) => "Meshlet view is off. The shop is the original triangle list.",
        };
        self.append(sentence);
    }

    fn sync_meshlet_debug(&mut self) -> bool {
        let active = self.meshlet_debug || self.meshlet_shade;
        if !active {
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.set_meshlet_mode(false, false);
            }
            return false;
        }
        let Some(entity) = self.selection.primary_entity() else {
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.set_meshlet_mode(false, false);
            }
            return false;
        };
        let playing = self.session_active();
        if self.meshlet_bound == Some((entity, playing)) {
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.set_meshlet_mode(self.meshlet_debug, self.meshlet_shade);
            }
            return false;
        }
        let Some(mesh_id) = self.viewed_world().extract(jarvig_core::RenderFrameId(1)).ok().and_then(|snapshot| {
            snapshot.instances().iter().find(|instance| instance.entity == entity).map(|instance| instance.mesh)
        }) else {
            return false;
        };
        let asset = self.engine.world().authored_mesh(entity).and_then(|(_, _, _, _, _, _, mesh, _)| match mesh {
            jarvig_core::MeshAssetRef::Asset { id, .. } => Some(id),
            _ => None,
        });
        let Some(asset) = asset else {
            self.meshlet_bound = Some((entity, playing));
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.set_meshlet_mode(false, false);
            }
            return false;
        };
        self.set_status("Building the meshlet debug view. The asset and the level are unchanged.");
        self.pump_loading();
        let Some(draw) = self.engine.mesh_asset_library().meshlets(asset).map(jarvig_core::meshlet_draw) else {
            self.meshlet_bound = Some((entity, playing));
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.set_meshlet_mode(false, false);
            }
            return false;
        };
        let batch = jarvig_renderer::MeshletDebugBatch {
            mesh: mesh_id,
            indices: draw.indices,
            colors: draw.colors,
            ranges: draw.ranges.into_iter().map(|range| (range.first_index, range.index_count)).collect(),
            spans: draw.spans.into_iter().map(|range| (range.first_index, range.index_count)).collect(),
            span_source: draw.span_source,
            owners: draw.owners,
            show_ids: self.meshlet_debug,
            shade_clustered: self.meshlet_shade,
        };
        if let Some(renderer) = self.renderer.as_mut() {
            if let Err(error) = renderer.set_meshlet_debug(Some(batch)) {
                self.append(&format!("Meshlet debug did not upload: {}.", describe(error)));
                self.meshlet_debug = false;
                self.meshlet_shade = false;
                self.meshlet_bound = None;
                self.sync_view_menu();
                return false;
            }
        }
        self.meshlet_bound = Some((entity, playing));
        true
    }

    fn gizmo_vertices(&self) -> Option<Vec<jarvig_renderer::OverlayVertex>> {
        let camera = self.editor_camera.as_ref()?;
        let mut vertices = Vec::new();
        if let (Some((_entity, pose)), Some(renderer)) = (self.gizmo_target(), self.renderer.as_ref()) {
            let height = renderer.configured_size().1;
            let distance = vec_len(vec_sub(pose.translation, camera.position));
            let length = gizmo::visual_length(distance, camera.vertical_fov_radians, height);
            vertices.extend(gizmo::build_gizmo(
                pose.translation,
                camera.position,
                length,
                self.transform_space,
                pose.rotation,
                self.object_tool == chrome::ToolbarCommand::Rotate,
                self.gizmo_hover.filter(|handle| handle.is_rotation() == (self.object_tool == chrome::ToolbarCommand::Rotate)),
                self.gizmo_drag.as_ref().map(|drag| drag.handle),
            ));
        }
        let segments = self.visible_joint_segments();
        if !segments.is_empty() {
            vertices.extend(gizmo::joint_debug_vertices(&segments, camera.position));
        }
        if vertices.is_empty() { None } else { Some(vertices) }
    }

    fn begin_capture(&mut self, hwnd: HWND, kind: CaptureKind) {
        if self.capture.is_some() {
            self.end_capture(true, false);
        }
        let _ = self.workspace.apply(WorkspaceCommand::Focus(PERSPECTIVE));
        self.text_focused = false;
        unsafe {
            SetFocus(hwnd);
            let mut point = POINT { x: 0, y: 0 };
            if GetCursorPos(&mut point) != 0 {
                self.capture_point = point;
            }
            SetCapture(hwnd);
        }
        if kind != CaptureKind::Gizmo && kind != CaptureKind::Terrain {
            self.hide_cursor();
        }
        self.capture = Some(kind);
        self.nav.capture = true;
        self.nav.look = kind == CaptureKind::Look;
        self.nav.pan = kind == CaptureKind::Pan;
        self.nav.orbit = kind == CaptureKind::Orbit;
    }

    fn end_capture(&mut self, release: bool, clear_keys: bool) {
        if self.capture == Some(CaptureKind::Gizmo) && self.gizmo_drag.is_some() {
            self.restore_gizmo_drag();
        }
        if self.capture == Some(CaptureKind::Terrain) {
            self.land_stamp = None;
            self.land_flatten = None;
        }
        let was_active = self.capture.take().is_some() || self.cursor_hidden;
        self.nav.look = false;
        self.nav.pan = false;
        self.nav.orbit = false;
        self.nav.capture = false;
        self.nav.mouse_dx = 0.0;
        self.nav.mouse_dy = 0.0;
        self.nav.wheel_notches = 0;
        if clear_keys {
            self.nav_keys = NavKeys::default();
            self.nav.forward = 0;
            self.nav.strafe = 0;
            self.nav.vertical = 0;
            self.nav.boost = false;
        }
        if !was_active {
            return;
        }
        self.show_cursor();
        if release {
            let point = self.capture_point;
            self.capture_ending = true;
            unsafe {
                ReleaseCapture();
                SetCursorPos(point.x, point.y);
            }
            self.capture_ending = false;
        }
    }

    fn hide_cursor(&mut self) {
        if self.cursor_hidden {
            return;
        }
        unsafe {
            while ShowCursor(0) >= 0 {}
        }
        self.cursor_hidden = true;
    }

    fn show_cursor(&mut self) {
        if !self.cursor_hidden {
            return;
        }
        unsafe {
            while ShowCursor(1) < 0 {}
        }
        self.cursor_hidden = false;
    }

    fn sample_captured_mouse(&mut self) {
        if self.capture.is_none() || matches!(self.capture, Some(CaptureKind::Gizmo | CaptureKind::Terrain)) {
            return;
        }
        unsafe {
            let mut point = POINT { x: 0, y: 0 };
            if GetCursorPos(&mut point) == 0 {
                return;
            }
            let dx = (point.x - self.capture_point.x) as f64;
            let dy = (point.y - self.capture_point.y) as f64;
            if dx != 0.0 || dy != 0.0 {
                self.nav.mouse_dx += dx;
                self.nav.mouse_dy += dy;
                SetCursorPos(self.capture_point.x, self.capture_point.y);
            }
        }
    }

    fn destroy_selected(&mut self) {
        if self.session_active() {
            self.append("Delete is disabled while playing. Stop returns to the authored world.");
            return;
        }
        let Some(target) = self.selection.primary_entity() else {
            return;
        };
        match self.engine.execute_authoring(AuthoringCommand::DestroyEntity { target }) {
            Ok(_) => {
                self.append(&format!("Destroyed {target}."));
                self.sync_outliner();
            }
            Err(error) => self.append(&format!("Destroy failed: {error}.")),
        }
    }

    fn duplicate_selected(&mut self) {
        if self.session_active() {
            self.append("Duplicate is disabled while playing. Stop returns to the authored world.");
            return;
        }
        let Some(target) = self.selection.primary_entity() else {
            return;
        };
        match self.engine.execute_authoring(AuthoringCommand::DuplicateEntity { target }) {
            Ok(jarvig_core::AuthoringResult::Duplicated(created)) => {
                self.append(&format!("Duplicated {target} as {created}."));
                self.sync_outliner();
            }
            Ok(_) => self.append("Duplicate did not return a new entity."),
            Err(error) => self.append(&format!("Duplicate failed: {error}.")),
        }
    }

    fn play_settings(&mut self) -> jarvig_core::GameSettings {
        let Some(project_file) = self.project_file.clone() else {
            return jarvig_core::GameSettings::inactive();
        };
        let Ok(project) = jarvig_core::load_project_file(&project_file) else {
            return jarvig_core::GameSettings::inactive();
        };
        match jarvig_core::load_project_game_settings(&project_file, &project) {
            Ok(settings) => settings,
            Err(error) => {
                self.append(&format!("Project settings were not used: {error}."));
                jarvig_core::GameSettings::inactive()
            }
        }
    }

    fn begin_play_capture(&mut self) {
        if self.play_captured {
            return;
        }
        self.end_capture(true, true);
        let hwnd = self.panel_hwnd(PERSPECTIVE);
        unsafe {
            SetFocus(hwnd);
            let mut point = POINT { x: 0, y: 0 };
            if GetCursorPos(&mut point) != 0 {
                self.capture_point = point;
            }
            SetCapture(hwnd);
        }
        self.hide_cursor();
        self.play_captured = true;
    }

    fn end_play_capture(&mut self) {
        if !self.play_captured {
            return;
        }
        self.play_captured = false;
        self.show_cursor();
        unsafe { ReleaseCapture(); }
    }

    fn sample_play_mouse(&mut self) {
        if !self.play_captured {
            return;
        }
        unsafe {
            let mut point = POINT { x: 0, y: 0 };
            if GetCursorPos(&mut point) == 0 {
                return;
            }
            let dx = (point.x - self.capture_point.x) as f64;
            let dy = (point.y - self.capture_point.y) as f64;
            if dx != 0.0 || dy != 0.0 {
                let device = self.play_control.device_mut();
                device.mouse_dx += dx;
                device.mouse_dy += dy;
                SetCursorPos(self.capture_point.x, self.capture_point.y);
            }
        }
    }

    fn play_key(wparam: WPARAM) -> Option<jarvig_core::PhysicalControl> {
        Some(match wparam {
            0x57 => jarvig_core::PhysicalControl::KeyW,
            0x41 => jarvig_core::PhysicalControl::KeyA,
            0x53 => jarvig_core::PhysicalControl::KeyS,
            0x44 => jarvig_core::PhysicalControl::KeyD,
            0x51 => jarvig_core::PhysicalControl::KeyQ,
            0x45 => jarvig_core::PhysicalControl::KeyE,
            0x20 => jarvig_core::PhysicalControl::Space,
            key if key == VK_SHIFT as usize => jarvig_core::PhysicalControl::Shift,
            key if key == VK_CONTROL as usize => jarvig_core::PhysicalControl::Control,
            key if key == VK_ESCAPE as usize => jarvig_core::PhysicalControl::Escape,
            _ => return None,
        })
    }

    fn on_nav_key(&mut self, message: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
        if self.session_active() && self.play_control.pawn().is_some() {
            if let Some(control) = Self::play_key(wparam) {
                let down = message == WM_KEYDOWN;
                if down && (lparam & 0x4000_0000) != 0 {
                    return true;
                }
                let device = self.play_control.device_mut();
                if down {
                    device.held.insert(control);
                } else {
                    device.held.remove(&control);
                }
                return true;
            }
        }
        if !self.navigation_open() {
            return false;
        }
        let down = message == WM_KEYDOWN;
        if down && (lparam & 0x4000_0000) != 0 {
            return true;
        }
        if wparam == VK_ESCAPE as usize {
            if self.capture.is_some() || self.gizmo_drag.is_some() {
                self.end_capture(true, true);
            }
            return true;
        }
        if down && self.capture.is_none() {
            match wparam {
                0x31 => {
                    self.object_tool = chrome::ToolbarCommand::Select;
                    unsafe { InvalidateRect(self.toolbar, std::ptr::null(), 0); }
                    return true;
                }
                0x32 => {
                    self.object_tool = chrome::ToolbarCommand::Translate;
                    unsafe { InvalidateRect(self.toolbar, std::ptr::null(), 0); }
                    return true;
                }
                0x33 => {
                    self.object_tool = chrome::ToolbarCommand::Rotate;
                    unsafe { InvalidateRect(self.toolbar, std::ptr::null(), 0); }
                    return true;
                }
                0x34 => {
                    self.append("Scale is unavailable. A spatial frame stores translation and a quaternion, not scale.");
                    return true;
                }
                _ => {}
            }
        }
        if down && wparam == VK_F && self.capture != Some(CaptureKind::Look) {
            let _ = self.focus_selected();
            return true;
        }
        if down && self.capture.is_none() {
            let ctrl = (unsafe { GetKeyState(VK_CONTROL as i32) } as u16) & 0x8000 != 0;
            if wparam == VK_DELETE {
                self.destroy_selected();
                return true;
            }
            if ctrl && (wparam == 0x5A || wparam == b'z' as usize) {
                self.undo_content_drop();
                return true;
            }
            if ctrl && wparam == VK_D {
                self.duplicate_selected();
                return true;
            }
        }
        let held = match wparam {
            VK_W => Some(&mut self.nav_keys.forward),
            VK_S => Some(&mut self.nav_keys.back),
            VK_A => Some(&mut self.nav_keys.left),
            VK_D => Some(&mut self.nav_keys.right),
            VK_Q => Some(&mut self.nav_keys.down),
            VK_E => Some(&mut self.nav_keys.up),
            key if key == VK_SHIFT as usize => Some(&mut self.nav_keys.boost),
            _ => None,
        };
        if let Some(slot) = held {
            *slot = down;
            return true;
        }
        false
    }

    fn on_viewport_input(&mut self, hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> bool {
        if self.session_active() && self.play_control.pawn().is_some() {
            match message {
                WM_LBUTTONDOWN => {
                    self.play_control.device_mut().held.insert(jarvig_core::PhysicalControl::MouseLeft);
                    if !self.play_captured {
                        self.begin_play_capture();
                    }
                    return true;
                }
                WM_LBUTTONUP => {
                    self.play_control.device_mut().held.remove(&jarvig_core::PhysicalControl::MouseLeft);
                    return true;
                }
                WM_MOUSEMOVE => {
                    self.sample_play_mouse();
                    return true;
                }
                WM_RBUTTONDOWN | WM_RBUTTONUP | WM_MBUTTONDOWN | WM_MBUTTONUP | WM_MOUSEWHEEL => return true,
                _ => {}
            }
        }
        if self.session_active()
            && matches!(
                message,
                WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP | WM_MBUTTONDOWN | WM_MBUTTONUP | WM_MOUSEWHEEL
            )
        {
            return true;
        }
        match message {
            WM_RBUTTONDOWN => {
                if self.gizmo_drag.is_some() {
                    return true;
                }
                self.begin_capture(hwnd, CaptureKind::Look);
                true
            }
            WM_RBUTTONUP => {
                if self.capture == Some(CaptureKind::Look) {
                    self.end_capture(true, false);
                }
                true
            }
            WM_MBUTTONDOWN => {
                if self.gizmo_drag.is_some() {
                    return true;
                }
                self.begin_capture(hwnd, CaptureKind::Pan);
                true
            }
            WM_MBUTTONUP => {
                if self.capture == Some(CaptureKind::Pan) {
                    self.end_capture(true, false);
                }
                true
            }
            WM_LBUTTONDOWN => {
                self.note_child_focus(PERSPECTIVE.raw());
                let alt = (unsafe { GetKeyState(VK_MENU as i32) } as u16) & 0x8000 != 0;
                if alt {
                    if self.gizmo_drag.is_none() {
                        self.begin_capture(hwnd, CaptureKind::Orbit);
                    }
                    return true;
                }
                if matches!(self.capture, Some(CaptureKind::Look | CaptureKind::Pan | CaptureKind::Orbit | CaptureKind::Terrain)) {
                    return true;
                }
                let x = (lparam & 0xffff) as u16 as i16 as f64;
                let y = ((lparam >> 16) & 0xffff) as u16 as i16 as f64;
                if self.land_brush().is_some() {
                    self.begin_capture(hwnd, CaptureKind::Terrain);
                    self.stamp_land(x, y, true);
                    return true;
                }
                let ctrl = (unsafe { GetKeyState(VK_CONTROL as i32) } as u16) & 0x8000 != 0;
                self.viewport_press(hwnd, x, y, ctrl);
                true
            }
            WM_LBUTTONUP => {
                if self.capture == Some(CaptureKind::Gizmo) {
                    self.commit_gizmo();
                } else if self.capture == Some(CaptureKind::Orbit) || self.capture == Some(CaptureKind::Terrain) {
                    self.end_capture(true, false);
                }
                true
            }
            WM_MOUSEMOVE => {
                let x = (lparam & 0xffff) as u16 as i16 as f64;
                let y = ((lparam >> 16) & 0xffff) as u16 as i16 as f64;
                if self.capture == Some(CaptureKind::Gizmo) {
                    self.update_gizmo_drag(x, y);
                } else if self.capture == Some(CaptureKind::Terrain) {
                    self.stamp_land(x, y, false);
                } else {
                    self.sample_captured_mouse();
                    self.update_gizmo_hover(x, y);
                    self.track_land_hover(x, y);
                }
                true
            }
            WM_MOUSEWHEEL => {
                if self.capture == Some(CaptureKind::Gizmo) {
                    return true;
                }
                if self.navigation_open() {
                    let raw = ((wparam >> 16) & 0xffff) as u16 as i16 as i32;
                    let notches = raw / 120;
                    if notches != 0 {
                        let alt = (unsafe { GetKeyState(VK_MENU as i32) } as u16) & 0x8000 != 0;
                        let camera_captured = matches!(self.capture, Some(CaptureKind::Look | CaptureKind::Pan | CaptureKind::Orbit));
                        if !alt && !camera_captured && self.land_radius_wheel() {
                            self.scale_land_radius(notches);
                        } else {
                            self.nav.wheel_notches = self.nav.wheel_notches.saturating_add(notches);
                            self.nav.alt = alt;
                        }
                    }
                }
                true
            }
            WM_KEYDOWN | WM_KEYUP => self.on_nav_key(message, wparam, lparam),
            WM_KILLFOCUS => {
                self.end_capture(true, true);
                true
            }
            WM_CAPTURECHANGED => {
                if !self.capture_ending && self.capture.is_some() {
                    self.end_capture(false, true);
                }
                true
            }
            WM_SETCURSOR => self.capture.is_some() && self.capture != Some(CaptureKind::Terrain),
            _ => false,
        }
    }

    fn probe_editor_camera(&mut self) -> Result<(), String> {
        let home = self.camera_home.ok_or("camera home missing")?;
        self.workspace.apply(WorkspaceCommand::Focus(INSPECTOR)).map_err(|error| error.to_string())?;
        self.text_focused = true;
        let parked = self.editor_camera.as_ref().ok_or("camera missing")?.position;
        self.nav_keys.forward = true;
        self.tick_editor_camera(0.5)?;
        if self.editor_camera.as_ref().ok_or("camera missing")?.position != parked {
            return Err("inspector text focus let W move the camera".into());
        }
        self.workspace.apply(WorkspaceCommand::Focus(PERSPECTIVE)).map_err(|error| error.to_string())?;
        self.text_focused = false;
        self.nav_keys = NavKeys::default();
        if key_route(Some(PERSPECTIVE), false) != dock::KeyRoute::Viewport(PERSPECTIVE) || key_route(Some(OUTPUT), true) == dock::KeyRoute::Viewport(OUTPUT) {
            return Err("viewport key route is wrong".into());
        }
        let world_before = self.engine.world().revision();
        let selection_before = self.selection.revision();
        let meshes = self.renderer.as_ref().map(|renderer| renderer.mesh_upload_count());
        let textures = self.renderer.as_ref().map(|renderer| renderer.texture_upload_count());
        let compiles = self.engine.material_compile_count();
        let lights_before = self.engine.world().extract(RenderFrameId(20)).map_err(|error| error.to_string())?;
        let point_before = lights_before.lights().iter().find(|light| light.kind == LightKind::Point).ok_or("point light missing")?.pose.translation;
        let forward_before = self.editor_camera.as_ref().ok_or("camera missing")?.forward();
        self.nav.look = true;
        self.nav.mouse_dx = 40.0;
        self.nav_keys.forward = false;
        self.tick_editor_camera(0.0)?;
        let forward_after_look = self.editor_camera.as_ref().ok_or("camera missing")?.forward();
        if vec_len(vec_sub(forward_after_look, forward_before)) < 0.05 {
            return Err("mouse look did not change orientation".into());
        }
        let speed = self.editor_camera.as_ref().ok_or("camera missing")?.speed_m_s;
        let before_step = self.editor_camera.as_ref().ok_or("camera missing")?.position;
        self.nav.look = false;
        self.nav_keys.forward = true;
        let step_dt = 0.05;
        self.tick_editor_camera(step_dt)?;
        let stepped = vec_len(vec_sub(self.editor_camera.as_ref().ok_or("camera missing")?.position, before_step));
        if (stepped - speed * step_dt).abs() > 1.0e-3 {
            return Err(format!("W did not move {speed} m/s * {step_dt} s, moved {stepped}"));
        }
        self.nav_keys.forward = true;
        self.nav.look = true;
        for _ in 0..100 {
            self.nav.mouse_dx = 0.15;
            self.tick_editor_camera(1.0 / 60.0)?;
        }
        self.nav.look = false;
        self.nav_keys = NavKeys::default();
        if self.engine.world().revision() != world_before || self.selection.revision() != selection_before {
            return Err("navigation revised the world or the selection".into());
        }
        if self.engine.material_compile_count() != compiles
            || self.renderer.as_ref().map(|renderer| renderer.mesh_upload_count()) != meshes
            || self.renderer.as_ref().map(|renderer| renderer.texture_upload_count()) != textures
        {
            return Err("navigation uploaded a mesh, a texture, or a material".into());
        }
        let speed_before = self.editor_camera.as_ref().ok_or("camera missing")?.speed_m_s;
        self.nav.wheel_notches = 1;
        self.nav.alt = false;
        self.tick_editor_camera(0.0)?;
        let speed_after = self.editor_camera.as_ref().ok_or("camera missing")?.speed_m_s;
        if speed_after <= speed_before || self.engine.world().revision() != world_before {
            return Err("wheel speed did not change, or it revised the world".into());
        }
        let origin = self.editor_camera.as_ref().ok_or("camera missing")?.position;
        let speed = self.editor_camera.as_ref().ok_or("camera missing")?.speed_m_s;
        self.nav_keys.forward = true;
        self.tick_editor_camera(0.01 / speed)?;
        self.nav_keys.forward = false;
        let moved = self.editor_camera.as_ref().ok_or("camera missing")?.position;
        let precise = vec_len(vec_sub(moved, origin));
        if (precise - 0.01).abs() > 1.0e-6 {
            return Err(format!("0.01 m was not preserved in the camera pose ({precise})"));
        }
        let rounded = Vec3::new(moved.x as f32 as f64, moved.y as f32 as f64, moved.z as f32 as f64);
        if vec_len(vec_sub(moved, rounded)) < 1.0 {
            return Err("camera position survived a float32 round trip; that is the jitter we forbid".into());
        }
        let near = self.engine.world().entity_outline().first().map(|row| row.uuid).ok_or("near entity missing")?;
        let selection_at_focus = {
            self.selection.replace(selection::SelectionItem::entity(near).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
            self.selection.revision()
        };
        if !self.focus_selected() {
            return Err("focus selected did not frame the near triangle".into());
        }
        if self.selection.revision() != selection_at_focus || self.selection.primary_entity() != Some(near) {
            return Err("focus selected changed the selection".into());
        }
        self.sync_selection_view();
        let framed = self.editor_camera.as_ref().ok_or("camera missing")?;
        let distance = vec_len(vec_sub(framed.position, framed.orbit_pivot));
        if (distance - framed.orbit_distance).abs() > 1.0e-3 || !distance.is_finite() {
            return Err("focus did not place the camera at the framing distance".into());
        }
        let orbit_distance = framed.orbit_distance;
        self.nav.orbit = true;
        self.nav.mouse_dx = 18.0;
        self.tick_editor_camera(0.0)?;
        self.nav.orbit = false;
        let orbited = self.editor_camera.as_ref().ok_or("camera missing")?;
        let after_orbit = vec_len(vec_sub(orbited.position, orbited.orbit_pivot));
        if (after_orbit - orbit_distance).abs() > 1.0e-3 || (orbited.orbit_distance - orbit_distance).abs() > 1.0e-6 {
            return Err("orbit changed the distance to the pivot".into());
        }
        let pan_from = orbited.position;
        let pivot_from = orbited.orbit_pivot;
        self.nav.pan = true;
        self.nav.mouse_dx = 12.0;
        self.tick_editor_camera(0.0)?;
        self.nav.pan = false;
        let panned = self.editor_camera.as_ref().ok_or("camera missing")?;
        let pan_delta = vec_len(vec_sub(panned.position, pan_from));
        let pivot_delta = vec_len(vec_sub(panned.orbit_pivot, pivot_from));
        if pan_delta < 1.0e-4 || (pan_delta - pivot_delta).abs() > 1.0e-4 {
            return Err("pan did not move the camera and the pivot together".into());
        }
        self.assert_drawable()?;
        let pose = panned.pose();
        let snapshot = self.engine.world().extract(RenderFrameId(21)).map_err(|error| error.to_string())?;
        let instance = snapshot.instances().iter().find(|instance| instance.entity == near).ok_or("near instance missing")?;
        let relative = camera_relative_f32(instance.pose.translation, pose.translation);
        if relative.iter().any(|component| !component.is_finite() || component.abs() > 500.0) {
            return Err(format!("gpu relative position left meter scale: {relative:?}"));
        }
        let point_after = snapshot.lights().iter().find(|light| light.kind == LightKind::Point).ok_or("point light missing")?.pose.translation;
        if point_after != point_before {
            return Err("navigation moved a world light".into());
        }
        let packet_home = render_light_record(lights_before.lights().iter().find(|light| light.kind == LightKind::Point).unwrap(), &home);
        let packet_now = render_light_record(snapshot.lights().iter().find(|light| light.kind == LightKind::Point).unwrap(), &pose);
        if packet_home.position == packet_now.position {
            return Err("per-view light packet did not change with the editor camera".into());
        }
        if packet_now.position.iter().any(|component| !component.is_finite() || component.abs() > 500.0) {
            return Err(format!("per-view light left meter scale: {:?}", packet_now.position));
        }
        let hidden_pose = pose;
        self.capture = Some(CaptureKind::Look);
        self.end_capture(false, true);
        if self.capture.is_some() || self.editor_camera.as_ref().ok_or("camera missing")?.pose() != hidden_pose {
            return Err("ending capture changed the pose or left capture active".into());
        }
        if self.workspace.close_panel(PERSPECTIVE).is_ok() {
            return Err("perspective became closable".into());
        }
        let (position, forward, speed, fov, mode, updates, focus_actions, frame) = {
            let controller = self.editor_camera.as_ref().ok_or("camera missing")?;
            (
                controller.position,
                controller.forward(),
                controller.speed_m_s,
                controller.vertical_fov_radians.to_degrees(),
                controller.mode.label(),
                controller.updates,
                controller.focus_actions,
                controller.reference_frame.0,
            )
        };
        let world_after = self.engine.world().revision();
        let selection_after = self.selection.revision();
        self.camera_report = format!(
            "JRV-0064 camera editor_camera_position={:.4},{:.4},{:.4} editor_camera_forward={:.4},{:.4},{:.4} editor_camera_speed={speed:.4} editor_camera_fov={fov:.4} editor_camera_mode={mode} camera_input_updates={updates} camera_focus_actions={focus_actions} camera_capture_active=0 world_revision_before_navigation={world_before} world_revision_after_navigation={world_after} selection_revision_after_focus={selection_after} reference_frame={frame}\n",
            position.x, position.y, position.z, forward.x, forward.y, forward.z
        );
        self.probe_gizmo()
    }

    fn probe_gizmo(&mut self) -> Result<(), String> {
        self.nav_keys = NavKeys::default();
        self.nav = camera::ViewportNavInput::default();
        self.reset_editor_camera()?;
        self.workspace.apply(WorkspaceCommand::Focus(PERSPECTIVE)).map_err(|error| error.to_string())?;
        self.text_focused = false;
        let hwnd = self.panel_hwnd(PERSPECTIVE);
        let (width, height) = self.renderer.as_ref().ok_or("renderer missing")?.configured_size();
        if width < 8 || height < 8 {
            return Err(format!("viewport too small to pick {width}x{height}"));
        }
        let meshes = self.renderer.as_ref().map(|renderer| renderer.mesh_upload_count());
        let textures = self.renderer.as_ref().map(|renderer| renderer.texture_upload_count());
        let compiles = self.engine.material_compile_count();
        let outline = self.engine.world().entity_outline();
        let near = outline.first().map(|row| row.uuid).ok_or("near entity missing")?;
        let far = outline.get(1).map(|row| row.uuid).ok_or("far entity missing")?;
        if self.outliner.display_name(near) != Some("Near Triangle") || self.outliner.display_name(far) != Some("Far Triangle") {
            return Err("bootstrap names changed before picking".into());
        }
        let original = self.local_translation_of(near)?;
        let original_rotation = self.local_rotation_of(near)?;
        let world_before_pick = self.engine.world().revision();
        self.selection.clear_from(selection::SelectionSource::Viewport);
        self.sync_selection_view();
        if !self.selection.is_empty() || !self.inspector_contains("No selection.") {
            return Err("clearing selection before the pick did not empty the inspector".into());
        }
        let selection_before_pick = self.selection.revision();
        self.object_tool = chrome::ToolbarCommand::Select;
        let center_x = width as f64 * 0.5 - 0.5;
        let center_y = height as f64 * 0.5 - 0.5;
        self.viewport_press(hwnd, center_x, center_y, false);
        if self.selection.primary_entity() != Some(near) || self.selection.count() != 1 {
            return Err(format!("center click selected {:?} count {}", self.selection.primary_entity(), self.selection.count()));
        }
        if self.engine.world().revision() != world_before_pick {
            return Err("viewport selection revised the world".into());
        }
        if self.selection.revision() == selection_before_pick {
            return Err("viewport selection did not revise the selection service".into());
        }
        if !self.inspector_contains("Near Triangle") || !self.inspector_contains("Mesh Actor") {
            return Err("inspector did not follow the viewport pick".into());
        }
        if !window_text(self.status).contains("Selected: Near Triangle") {
            return Err("status did not follow the viewport pick".into());
        }
        let scale = self.toolbar_index(chrome::ToolbarCommand::Scale)?;
        self.toolbar_click(scale);
        if self.object_tool != chrome::ToolbarCommand::Select || !self.log.contains("Scale is unavailable") {
            return Err("scale pretended to become a tool".into());
        }
        self.object_tool = chrome::ToolbarCommand::Translate;
        if self.gizmo_vertices().is_none() {
            return Err("translate did not build a gizmo".into());
        }
        let pose = self.engine.world().entity_world_pose(near).map_err(|error| error.to_string())?;
        let length = self.gizmo_length(pose.translation).ok_or("gizmo length missing")?;
        let (axis_x, axis_y) = self.pixel_for_world_point(pose.translation + Vec3::new(length * 0.45, 0.0, 0.0))?;
        if self.gizmo_handle_at(axis_x, axis_y) != Some(gizmo::GizmoHandle::AxisX) {
            return Err(format!("x axis hit {:?} at {axis_x:.1},{axis_y:.1}", self.gizmo_handle_at(axis_x, axis_y)));
        }
        let selection_during = self.selection.revision();
        self.viewport_press(hwnd, axis_x, axis_y, false);
        if self.gizmo_drag.as_ref().map(|drag| drag.handle) != Some(gizmo::GizmoHandle::AxisX) {
            return Err("x axis press did not begin a drag".into());
        }
        self.apply_axis_delta(0.01)?;
        let centimeter = self.local_translation_of(near)?;
        if (centimeter.x - (original.x + 0.01)).abs() > 1.0e-12
            || (centimeter.y - original.y).abs() > 1.0e-12
            || (centimeter.z - original.z).abs() > 1.0e-12
        {
            return Err(format!("0.01 m was not preserved ({centimeter:?})"));
        }
        let world_pose = self.engine.world().entity_world_pose(near).map_err(|error| error.to_string())?;
        if (world_pose.translation.x - (jarvig_core::BOOTSTRAP_ROOT_M + original.x + 0.01)).abs() > 1.0e-6 {
            return Err("large-world translation lost the centimeter".into());
        }
        let camera_position = self.editor_camera.as_ref().ok_or("camera missing")?.position;
        let relative = camera_relative_f32(world_pose.translation, camera_position);
        if relative.iter().any(|component| !component.is_finite() || component.abs() > 20.0) {
            return Err(format!("translated instance left meter scale: {relative:?}"));
        }
        let gizmo_now = self.gizmo_vertices().ok_or("gizmo disappeared during the drag")?;
        if gizmo_now.iter().any(|vertex| vertex.position.iter().any(|component| !component.is_finite() || component.abs() > 50.0)) {
            return Err("gizmo vertices were not camera-relative".into());
        }
        self.cancel_gizmo();
        if self.local_translation_of(near)? != original || self.local_rotation_of(near)? != original_rotation {
            return Err("cancel did not restore the original transform".into());
        }
        if self.gizmo_drag.is_some() || self.capture.is_some() {
            return Err("cancel left a drag or a capture".into());
        }
        if self.selection.revision() != selection_during || self.selection.primary_entity() != Some(near) {
            return Err("cancel changed the selection".into());
        }
        self.toolbar_click(self.toolbar_index(chrome::ToolbarCommand::Local)?);
        if self.transform_space != gizmo::TransformSpace::Local {
            return Err("local toggle did not enter local space".into());
        }
        self.viewport_press(hwnd, axis_x, axis_y, false);
        if self.gizmo_drag.as_ref().map(|drag| drag.handle) != Some(gizmo::GizmoHandle::AxisX) {
            return Err("local x axis did not begin a drag".into());
        }
        self.nudge_gizmo(0.25)?;
        let local_moved = self.local_translation_of(near)?;
        if (local_moved.x - (original.x + 0.25)).abs() > 1.0e-4
            || (local_moved.y - original.y).abs() > 1.0e-4
            || (local_moved.z - original.z).abs() > 1.0e-4
        {
            return Err(format!("local x drag changed another axis ({local_moved:?})"));
        }
        self.cancel_gizmo();
        if self.local_translation_of(near)? != original {
            return Err("local cancel did not restore translation".into());
        }
        self.toolbar_click(self.toolbar_index(chrome::ToolbarCommand::Local)?);
        if self.transform_space != gizmo::TransformSpace::World {
            return Err("local toggle did not return to world space".into());
        }
        self.viewport_press(hwnd, axis_x, axis_y, false);
        self.nudge_gizmo(0.25)?;
        if self.engine.world().revision() == world_before_pick {
            return Err("translation drag did not revise the world".into());
        }
        if self.selection.revision() != selection_during {
            return Err("translation drag revised the selection".into());
        }
        let dragged = self.local_translation_of(near)?;
        if (dragged.x - (original.x + 0.25)).abs() > 1.0e-4
            || (dragged.y - original.y).abs() > 1.0e-4
            || (dragged.z - original.z).abs() > 1.0e-4
        {
            return Err(format!("world x drag was not axis aligned ({dragged:?})"));
        }
        self.cancel_gizmo();
        if self.local_translation_of(near)? != original {
            return Err("cancel did not restore the 0.25 m drag".into());
        }
        let world_before_far = self.engine.world().revision();
        let camera_pose = self.editor_camera.as_ref().ok_or("camera missing")?.pose().translation;
        let far_point = Vec3::new(camera_pose.x + 1.8, camera_pose.y - 1.2, camera_pose.z - 5.0);
        let (far_x, far_y) = self.pixel_for_world_point(far_point)?;
        self.object_tool = chrome::ToolbarCommand::Select;
        self.viewport_press(hwnd, far_x, far_y, true);
        if self.selection.count() != 2
            || self.selection.primary_entity() != Some(far)
            || !self.selection.contains(selection::SelectionItem::Entity(near))
        {
            return Err(format!(
                "ctrl click did not add the far triangle (count {} primary {:?})",
                self.selection.count(),
                self.selection.primary_entity()
            ));
        }
        if self.engine.world().revision() != world_before_far {
            return Err("ctrl click revised the world".into());
        }
        self.object_tool = chrome::ToolbarCommand::Translate;
        if self.gizmo_target().map(|(entity, _)| entity) != Some(far) {
            return Err("multi-select gizmo did not follow the primary".into());
        }
        self.object_tool = chrome::ToolbarCommand::Select;
        self.viewport_press(hwnd, 0.0, height as f64 - 1.0, true);
        if self.selection.count() != 2 {
            return Err("ctrl click on empty space changed the selection".into());
        }
        self.viewport_press(hwnd, 0.0, height as f64 - 1.0, false);
        if !self.selection.is_empty() || self.gizmo_vertices().is_some() {
            return Err("empty click did not clear selection".into());
        }
        self.viewport_press(hwnd, center_x, center_y, false);
        if self.selection.primary_entity() != Some(near) || self.selection.count() != 1 {
            return Err("second center click did not select the near triangle".into());
        }
        self.object_tool = chrome::ToolbarCommand::Translate;
        let selection_at_commit = self.selection.revision();
        self.viewport_press(hwnd, axis_x, axis_y, false);
        if self.gizmo_drag.is_none() {
            return Err("commit drag did not start".into());
        }
        self.apply_axis_delta(0.50)?;
        self.commit_gizmo();
        let committed = self.local_translation_of(near)?;
        if (committed.x - (original.x + 0.50)).abs() > 1.0e-6
            || (committed.y - original.y).abs() > 1.0e-9
            || (committed.z - original.z).abs() > 1.0e-9
        {
            return Err(format!("commit did not keep +0.50 m ({committed:?})"));
        }
        if self.gizmo_drag.is_some() || self.capture.is_some() {
            return Err("commit left the drag active".into());
        }
        if self.selection.revision() != selection_at_commit || self.selection.primary_entity() != Some(near) {
            return Err("commit changed the selection".into());
        }
        if self.outliner.display_name(near) != Some("Near Triangle") {
            return Err("transform renamed the outliner row".into());
        }
        if !self.inspector_contains("Near Triangle") || !self.inspector_contains("0.5") {
            return Err("inspector did not observe the committed translation".into());
        }
        self.object_tool = chrome::ToolbarCommand::Rotate;
        let pose = self.engine.world().entity_world_pose(near).map_err(|error| error.to_string())?;
        let length = self.gizmo_length(pose.translation).ok_or("rotate length missing")?;
        let radius = length * 0.85;
        let ring_point = pose.translation + Vec3::new(radius * std::f64::consts::FRAC_1_SQRT_2, radius * std::f64::consts::FRAC_1_SQRT_2, 0.0);
        let (ring_x, ring_y) = self.pixel_for_world_point(ring_point)?;
        if self.gizmo_handle_at(ring_x, ring_y) != Some(gizmo::GizmoHandle::RingZ) {
            return Err(format!("z ring hit {:?}", self.gizmo_handle_at(ring_x, ring_y)));
        }
        self.viewport_press(hwnd, ring_x, ring_y, false);
        if self.gizmo_drag.as_ref().map(|drag| drag.handle) != Some(gizmo::GizmoHandle::RingZ) {
            return Err("z ring press did not begin a rotation".into());
        }
        self.spin_gizmo(0.2)?;
        let turned = self.local_rotation_of(near)?;
        let length_q = (turned.x * turned.x + turned.y * turned.y + turned.z * turned.z + turned.w * turned.w).sqrt();
        if (length_q - 1.0).abs() > 1.0e-6 || turned == original_rotation {
            return Err(format!("rotation was not a new unit quaternion ({turned:?})"));
        }
        if (self.local_translation_of(near)?.x - committed.x).abs() > 1.0e-6 {
            return Err("rotation drag changed translation".into());
        }
        self.cancel_gizmo();
        if self.local_rotation_of(near)? != original_rotation {
            return Err("rotation cancel did not restore the quaternion".into());
        }
        if self.local_translation_of(near)? != committed {
            return Err("rotation cancel changed the committed translation".into());
        }
        self.object_tool = chrome::ToolbarCommand::Translate;
        if self.gizmo_vertices().is_none() {
            return Err("the presented frame has no gizmo".into());
        }
        self.gizmo_restore = Some((near, original, original_rotation));
        if self.engine.material_compile_count() != compiles
            || self.renderer.as_ref().map(|renderer| renderer.mesh_upload_count()) != meshes
            || self.renderer.as_ref().map(|renderer| renderer.texture_upload_count()) != textures
        {
            return Err("gizmo manipulation uploaded a mesh, a texture, or a material".into());
        }
        if self.viewport_view != self.view_born || self.panel_hwnd(PERSPECTIVE) != self.viewport_born {
            return Err("gizmo manipulation recreated the viewport".into());
        }
        self.assert_drawable()?;
        if self.capture.is_some() || self.gizmo_drag.is_some() {
            return Err("probe left a gesture active".into());
        }
        println!(
            "JRV-0065 probe pick=PASS centimeter=PASS translate_cancel=PASS translate_commit=PASS rotate_cancel=PASS far=PASS empty=PASS scale=UNAVAILABLE local=PASS primary_only=PASS"
        );
        Ok(())
    }

    fn restore_presented_gizmo(&mut self) -> Result<(), String> {
        let Some((entity, local, rotation)) = self.gizmo_restore.take() else {
            return Err("presented gizmo restore was not armed".into());
        };
        self.engine
            .execute_authoring(AuthoringCommand::SetProperty {
                target: entity,
                type_id: jarvig_core::TYPE_SPATIAL_FRAME,
                field: jarvig_core::FIELD_LOCAL_TRANSLATION,
                value: PropertyValue::Vec3(local),
            })
            .map_err(|error| error.to_string())?;
        self.engine
            .execute_authoring(AuthoringCommand::SetProperty {
                target: entity,
                type_id: jarvig_core::TYPE_SPATIAL_FRAME,
                field: jarvig_core::FIELD_LOCAL_ROTATION,
                value: PropertyValue::Quat(rotation),
            })
            .map_err(|error| error.to_string())?;
        self.sync_outliner();
        if self.local_translation_of(entity)? != local || self.local_rotation_of(entity)? != rotation {
            return Err("presented frame restore did not return the bootstrap transform".into());
        }
        if self.selection.primary_entity() != Some(entity) {
            return Err("restoring the presented transform changed the selection".into());
        }
        Ok(())
    }

    fn local_translation_of(&self, entity: EntityUuid) -> Result<Vec3, String> {
        let inspection = self.engine.world().inspect_entity(entity).map_err(|error| error.to_string())?;
        match inspection.sections.get(1).and_then(|section| section.fields.first()).map(|field| field.value.clone()) {
            Some(PropertyValue::Vec3(value)) => Ok(value),
            _ => Err("local translation missing".into()),
        }
    }

    fn local_rotation_of(&self, entity: EntityUuid) -> Result<Quat, String> {
        let inspection = self.engine.world().inspect_entity(entity).map_err(|error| error.to_string())?;
        match inspection.sections.get(1).and_then(|section| section.fields.get(1)).map(|field| field.value.clone()) {
            Some(PropertyValue::Quat(value)) => Ok(value),
            _ => Err("local rotation missing".into()),
        }
    }

    fn toolbar_index(&self, command: chrome::ToolbarCommand) -> Result<usize, String> {
        for index in 0..32 {
            if self.toolbar_icons.command(index) == Some(command) {
                return Ok(index);
            }
        }
        Err("toolbar button missing".into())
    }

    /// Selected joint only, unless View > Show All Joints is checked. Level and Land stay clear of them.
    fn visible_joint_segments(&self) -> Vec<jarvig_core::JointDebugSegment> {
        if self.bind_pose_dir.is_none() && !self.character_workspace {
            return Vec::new();
        }
        if !self.show_joint_debug && !self.show_all_joints {
            return Vec::new();
        }
        let selected = self.selection.primary_entity();
        let limits = if self.show_joint_limits { selected } else { None };
        let mut segments = self.engine.world().joint_debug_segments(limits);
        if self.show_all_joints {
            return segments;
        }
        match selected {
            Some(id) => segments.retain(|segment| segment.entity == id),
            None => segments.clear(),
        }
        segments
    }

    fn joint_pivot_at(&self, x: f64, y: f64) -> Option<jarvig_core::EntityId> {
        let mut best: Option<(jarvig_core::EntityId, f64)> = None;
        for segment in self.visible_joint_segments() {
            if segment.limits {
                continue;
            }
            let Ok((px, py)) = self.pixel_for_world_point(segment.start) else { continue };
            let distance = (px - x).hypot(py - y);
            if distance > 14.0 {
                continue;
            }
            if best.as_ref().is_none_or(|(_, nearest)| distance < *nearest) {
                best = Some((segment.entity, distance));
            }
        }
        best.map(|(entity, _)| entity)
    }

    fn pixel_for_world_point(&self, point: Vec3) -> Result<(f64, f64), String> {
        let camera = self.editor_camera.as_ref().ok_or("camera missing")?;
        let (width, height) = self.renderer.as_ref().ok_or("renderer missing")?.configured_size();
        if width == 0 || height == 0 {
            return Err("viewport has no pixels".into());
        }
        let pose = camera.pose();
        let forward = pose.rotation.rotate(Vec3::new(0.0, 0.0, -1.0));
        let right = pose.rotation.rotate(Vec3::new(1.0, 0.0, 0.0));
        let up = pose.rotation.rotate(Vec3::new(0.0, 1.0, 0.0));
        let delta = vec_sub(point, pose.translation);
        let depth = vec_dot(delta, forward);
        if depth < 1.0e-4 {
            return Err("point is behind the camera".into());
        }
        let half = (camera.vertical_fov_radians * 0.5).tan();
        let aspect = width as f64 / height as f64;
        let ndc_x = vec_dot(delta, right) / (depth * half * aspect);
        let ndc_y = vec_dot(delta, up) / (depth * half);
        if !ndc_x.is_finite() || !ndc_y.is_finite() || ndc_x.abs() > 1.0 || ndc_y.abs() > 1.0 {
            return Err(format!("point is outside the view ({ndc_x}, {ndc_y})"));
        }
        Ok((((ndc_x + 1.0) * 0.5) * width as f64 - 0.5, ((1.0 - ndc_y) * 0.5) * height as f64 - 0.5))
    }

    fn nudge_gizmo(&mut self, meters: f64) -> Result<(), String> {
        let drag = self.gizmo_drag.ok_or("gizmo drag is not active")?;
        let along = vec_dot(vec_sub(drag.start_hit, drag.origin), drag.axis);
        self.apply_drag_hit(drag.origin + drag.axis.scale(along + meters));
        Ok(())
    }

    /// Exact axis delta. The ray constraint approximates this scalar. The write does not add it to the world origin.
    fn apply_axis_delta(&mut self, meters: f64) -> Result<(), String> {
        let drag = self.gizmo_drag.ok_or("gizmo drag is not active")?;
        self.write_translation_delta(drag.entity, drag.original_local, drag.axis.scale(meters));
        self.gizmo_updates = self.gizmo_updates.saturating_add(1);
        Ok(())
    }

    fn spin_gizmo(&mut self, radians: f64) -> Result<(), String> {
        let drag = self.gizmo_drag.ok_or("gizmo drag is not active")?;
        let spin = Quat::from_axis_angle(drag.axis, radians).map_err(|error| error.to_string())?;
        let offset = vec_sub(drag.start_hit, drag.origin);
        self.apply_drag_hit(drag.origin + spin.rotate(offset));
        Ok(())
    }

    fn append(&mut self, line: &str) {
        self.log.push_str(line);
        self.log.push_str("\r\n");
        let text = wide(&self.log);
        unsafe { SetWindowTextW(self.panel_hwnd(OUTPUT), text.as_ptr()); }
    }

    fn set_status(&self, text: &str) {
        if self.status.is_null() || window_text(self.status) == text {
            return;
        }
        let text = wide(text);
        unsafe {
            SetWindowTextW(self.status, text.as_ptr());
            InvalidateRect(self.status, std::ptr::null(), 0);
        }
    }

    fn on_command(&mut self, id: usize, notify: u32) {
        if notify == EN_SETFOCUS || notify == LBN_SETFOCUS {
            self.note_child_focus(id as u32);
            return;
        }
        if self.load_depth > 0 && id != ID_FILE_EXIT {
            return;
        }
        match id {
            ID_FILE_NEW_PROJECT | ID_FILE_TEMPLATE_EMPTY => self.file_new_project(),
            ID_FILE_TEMPLATE_TERRAIN => self.file_new_from_template("terrain"),
            ID_FILE_TEMPLATE_THIRD => self.file_new_from_template("third-person"),
            ID_FILE_TEMPLATE_FPS => self.file_new_from_template("fps"),
            ID_FILE_OPEN_PROJECT => self.file_open_project(),
            ID_FILE_CLOSE_PROJECT => self.file_close_project(),
            ID_FILE_NEW_LEVEL => self.file_new_level(),
            ID_FILE_OPEN_LEVEL => self.file_open_level(),
            ID_FILE_RELOAD => self.file_reload_level(),
            ID_FILE_SAVE => self.file_save(),
            ID_FILE_SAVE_AS => self.file_save_as(),
            ID_FILE_SAVE_ALL => self.file_save_all(),
            ID_FILE_IMPORT_MESH => self.file_import_mesh(),
            ID_EDIT_UNDO => self.undo_content_drop(),
            id if (ID_FILE_RECENT_PROJECT..ID_FILE_RECENT_PROJECT + 8).contains(&id) => self.file_recent_project(id - ID_FILE_RECENT_PROJECT),
            id if (ID_FILE_RECENT_LEVEL..ID_FILE_RECENT_LEVEL + 8).contains(&id) => self.file_recent_level(id - ID_FILE_RECENT_LEVEL),
            ID_FILE_EXIT => unsafe { PostMessageW(self.frame, WM_CLOSE, 0, 0); },
            ID_PLAY => self.begin_play(),
            ID_PLAY_STANDALONE => self.run_standalone(),
            ID_BUILD_PROJECT => self.build_project(false),
            ID_BUILD_AND_RUN => self.build_project(true),
            ID_BUILD_SETTINGS => self.build_settings(),
            ID_HELP_ABOUT => {
                let text = wide(
                    "JARVIGEditor hosts the JARVIG engine.\nThe viewport is one RenderView.\nLayout is a dock workspace, not engine state.",
                );
                let title = wide("JARVIGEditor");
                unsafe { MessageBoxW(self.frame, text.as_ptr(), title.as_ptr(), MB_OK); }
            }
            ID_VIEW_OUTLINER => self.toggle_panel(OUTLINER),
            ID_VIEW_CHARACTER => self.set_character_workspace(!self.character_workspace),
            ID_VIEW_LAND => self.set_land_mode(!self.land_mode),
            ID_VIEW_RESET_POSE => self.reset_character_pose(),
            ID_VIEW_JOINTS => {
                if !self.character_workspace {
                    self.append("Joint helpers draw in the Character workspace. Level stays clear of them.");
                } else {
                    self.show_joint_debug = !self.show_joint_debug;
                    if !self.show_joint_debug {
                        self.show_all_joints = false;
                    }
                    self.persist_workspace();
                }
                self.sync_view_menu();
                self.refresh_status();
            }
            ID_VIEW_ALL_JOINTS => {
                if !self.character_workspace {
                    self.append("Joint helpers draw in the Character workspace. Level stays clear of them.");
                } else {
                    self.show_all_joints = !self.show_all_joints;
                    if self.show_all_joints {
                        self.show_joint_debug = true;
                    }
                    self.persist_workspace();
                }
                self.sync_view_menu();
                self.refresh_status();
            }
            ID_VIEW_JOINT_LIMITS => {
                if !self.character_workspace {
                    self.append("Joint limits draw in the Character workspace. Level stays clear of them.");
                } else {
                    self.show_joint_limits = !self.show_joint_limits;
                    self.persist_workspace();
                }
                self.sync_view_menu();
                self.refresh_status();
            }
            ID_VIEW_INSPECTOR => self.toggle_panel(INSPECTOR),
            ID_VIEW_CONTENT => self.toggle_panel(CONTENT),
            ID_VIEW_OUTPUT => self.toggle_panel(OUTPUT),
            ID_VIEW_RESET => self.reset_from_menu(),
            ID_VIEW_EXPOSURE_UP => {
                let _ = self.nudge_exposure(1.0);
            }
            ID_VIEW_EXPOSURE_DOWN => {
                let _ = self.nudge_exposure(-1.0);
            }
            ID_VIEW_EXPOSURE_RESET => {
                let _ = self.set_exposure_ev(0.0);
            }
            ID_VIEW_CREATE_CAMERA => self.create_camera_actor(),
            ID_VIEW_STARTUP_CAMERA => self.set_startup_camera(),
            ID_VIEW_PILOT => self.toggle_pilot_camera(),
            ID_VIEW_MESHLETS => self.toggle_meshlet_debug(false),
            ID_VIEW_MESHLET_SHADE => self.toggle_meshlet_debug(true),
            ID_VIEW_MESHLET_FRUSTUM => self.toggle_meshlet_frustum(),
            ID_VIEW_MESHLET_OCCLUSION => self.toggle_meshlet_occlusion(),
            ID_VIEW_MESHLET_FREEZE => self.toggle_meshlet_freeze(),
            ID_VIEW_MESHLET_HIGHLIGHT => self.toggle_meshlet_highlight(),
            ID_VIEW_CLUSTER_HIERARCHY => self.toggle_cluster_hierarchy(),
            ID_VIEW_EINSTEIN => self.toggle_einstein_debug(),
            ID_VIEW_MICRO => self.toggle_microgeometry(),
            ID_VIEW_MICRO_COLOR => self.toggle_micro_color(),
            ID_VIEW_ERROR_HALF => self.set_hierarchy_error(0.5),
            ID_VIEW_ERROR_ONE => self.set_hierarchy_error(1.0),
            ID_VIEW_ERROR_TWO => self.set_hierarchy_error(2.0),
            ID_VIEW_ERROR_FOUR => self.set_hierarchy_error(4.0),
            ID_VIEW_BACKGROUND_JOBS => self.show_background_jobs(),
            ID_VIEW_CANCEL_JOB => self.cancel_background_job(),
            ID_VIEW_ENVIRONMENT => {
                let enabled = self.engine.world().environment().enabled;
                let _ = self.engine.world_mut().set_environment_enabled(!enabled);
                self.sync_view_menu();
                self.refresh_status();
            }
            ID_DEBUG_FULL => {
                let _ = self.apply_lighting_debug(LightingDebug::default());
            }
            ID_DEBUG_DIRECT => {
                let _ = self.apply_lighting_debug(LightingDebug::direct_only());
            }
            ID_DEBUG_ENV_DIFFUSE => {
                let _ = self.apply_lighting_debug(LightingDebug::environment_diffuse_only());
            }
            ID_DEBUG_ENV_SPECULAR => {
                let _ = self.apply_lighting_debug(LightingDebug::environment_specular_only());
            }
            ID_DEBUG_PROBE_ONLY => {
                let _ = self.apply_lighting_debug(LightingDebug::probe_specular_only());
            }
            ID_DEBUG_EMISSIVE => {
                let _ = self.apply_lighting_debug(LightingDebug::emissive_only());
            }
            ID_DEBUG_DIRECTIONAL => {
                let mut debug = self.lighting_debug;
                debug.directional = !debug.directional;
                let _ = self.apply_lighting_debug(debug);
            }
            ID_DEBUG_POINT => {
                let mut debug = self.lighting_debug;
                debug.point = !debug.point;
                let _ = self.apply_lighting_debug(debug);
            }
            ID_DEBUG_SPOT => {
                let mut debug = self.lighting_debug;
                debug.spot = !debug.spot;
                let _ = self.apply_lighting_debug(debug);
            }
            ID_DEBUG_GLOBAL_ENV => {
                let mut debug = self.lighting_debug;
                let enable = !(debug.env_diffuse || debug.env_specular);
                debug.env_diffuse = enable;
                debug.env_specular = enable;
                let _ = self.apply_lighting_debug(debug);
            }
            ID_DEBUG_PROBE => {
                let mut debug = self.lighting_debug;
                debug.probe = !debug.probe;
                let _ = self.apply_lighting_debug(debug);
            }
            ID_DEBUG_DIRECTIONAL_ONLY => {
                let _ = self.apply_lighting_debug(LightingDebug::directional_only());
            }
            ID_DEBUG_POINT_ONLY => {
                let _ = self.apply_lighting_debug(LightingDebug::point_only());
            }
            ID_DEBUG_SPOT_ONLY => {
                let _ = self.apply_lighting_debug(LightingDebug::spot_only());
            }
            ID_DEBUG_NO_SHADOWS => {
                let _ = self.apply_lighting_debug(LightingDebug::shadows_disabled());
            }
            ID_DEBUG_INDIRECT_ONLY => {
                let _ = self.apply_lighting_debug(LightingDebug::indirect_diffuse_only());
            }
            ID_DEBUG_DIRECT_UNSHADOWED => {
                let _ = self.apply_lighting_debug(LightingDebug::direct_unshadowed());
            }
            ID_DEBUG_CASCADES => {
                self.cascade_debug = !self.cascade_debug;
                let _ = self.apply_lighting_debug(self.lighting_debug);
                self.append("Shadow cascade tint is a view. It is not saved in the level.");
            }
            ID_DEBUG_CONTACT => {
                self.contact_shadows = !self.contact_shadows;
                let _ = self.apply_lighting_debug(self.lighting_debug);
                self.append(if self.contact_shadows {
                    "Screen-space contact shadows are on. They do not replace the shadow maps."
                } else {
                    "Screen-space contact shadows are off."
                });
            }
            ID_MAT_FULL => self.set_material_channel(0),
            ID_MAT_BASE => self.set_material_channel(1),
            ID_MAT_NORMAL => self.set_material_channel(2),
            ID_MAT_ROUGH => self.set_material_channel(3),
            ID_MAT_AO => self.set_material_channel(4),
            ID_MAT_METAL => self.set_material_channel(5),
            ID_VIEW_RECAPTURE => {
                self.engine.world_mut().request_probe_recapture();
                self.append("Reflection probes marked for recapture. The camera was not moved.");
                self.refresh_status();
            }
            ID_PROBE_STATIC => {
                self.engine.world_mut().set_probe_update_policy(jarvig_core::ProbeUpdatePolicy::Static);
                self.note_policy_edit();
            }
            ID_PROBE_ON_DEMAND => {
                self.engine.world_mut().set_probe_update_policy(jarvig_core::ProbeUpdatePolicy::OnDemand);
                self.note_policy_edit();
            }
            ID_PROBE_ON_TRANSFORM => {
                self.engine.world_mut().set_probe_update_policy(jarvig_core::ProbeUpdatePolicy::OnTransformChange);
                self.note_policy_edit();
            }
            ID_PROBE_ON_LIGHTING => {
                self.engine.world_mut().set_probe_update_policy(jarvig_core::ProbeUpdatePolicy::OnLightingChange);
                self.note_policy_edit();
            }
            ID_PROBE_TIME_SLICED => {
                self.engine.world_mut().set_probe_update_policy(jarvig_core::ProbeUpdatePolicy::TimeSliced);
                self.note_policy_edit();
                self.append("Probe updates are time sliced. One face per frame. The camera does not dirty a probe.");
                self.refresh_status();
            }
            ID_PROBE_RES_32 => self.set_probe_capture_resolution(32),
            ID_PROBE_RES_64 => self.set_probe_capture_resolution(64),
            ID_PROBE_RES_128 => self.set_probe_capture_resolution(128),
            ID_PROBE_RES_256 => self.set_probe_capture_resolution(256),
            ID_QUALITY_BASELINE => self.set_render_quality(jarvig_core::RenderQuality::Baseline),
            ID_QUALITY_ENHANCED => self.set_render_quality(jarvig_core::RenderQuality::Enhanced),
            ID_QUALITY_HIGH => self.set_render_quality(jarvig_core::RenderQuality::High),
            ID_PRESENT_DITHER => {
                self.output_dither = !self.output_dither;
                let _ = self.push_view_settings();
                self.append(if self.output_dither {
                    "Presentation dither is on. Triangular noise, not a diagonal weave. Not saved."
                } else {
                    "Presentation dither is off. Quantization steps are left visible. Not saved."
                });
            }
            ID_PRESENT_TONEMAP => self.set_presentation(PresentationMode::Tonemap),
            ID_PRESENT_QUANTIZED => self.set_presentation(PresentationMode::Quantized),
            ID_PRESENT_BEFORE => self.set_presentation(PresentationMode::BeforeCurve),
            _ => {}
        }
    }

    fn ensure_font(&mut self) {
        if self.font_dpi == self.dpi && !self.ui_font.is_null() {
            return;
        }
        if !self.ui_font.is_null() {
            unsafe { DeleteObject(self.ui_font as _); }
        }
        self.ui_font = chrome::ui_font(self.dpi);
        self.font_dpi = self.dpi;
        unsafe {
            for hwnd in [
                self.status,
                self.panel_hwnd(OUTLINER),
                self.panel_hwnd(INSPECTOR),
                self.panel_hwnd(CONTENT),
                self.panel_hwnd(OUTPUT),
            ] {
                if !hwnd.is_null() {
                    SendMessageW(hwnd, WM_SETFONT, self.ui_font as WPARAM, 1);
                }
            }
            for control in &self.inspector_controls {
                SendMessageW(control.hwnd, WM_SETFONT, self.ui_font as WPARAM, 1);
            }
            InvalidateRect(self.toolbar, std::ptr::null(), 1);
        }
    }

    fn paint_toolbar(&self, hwnd: HWND) {
        unsafe {
            let mut paint: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut paint);
            if !hdc.is_null() {
                let (width, height) = client_size(hwnd);
                self.toolbar_icons.paint(
                    hdc,
                    width as i32,
                    height as i32,
                    self.dpi,
                    self.toolbar_hot,
                    self.object_tool,
                    self.transform_space == gizmo::TransformSpace::Local,
                    self.play_toolbar(),
                    self.ui_font,
                    self.editor_mode(),
                );
            }
            EndPaint(hwnd, &paint);
        }
    }

    fn toolbar_click(&mut self, index: usize) {
        let Some(command) = self.toolbar_icons.command(index) else { return };
        match command {
            chrome::ToolbarCommand::Select | chrome::ToolbarCommand::Translate | chrome::ToolbarCommand::Rotate => {
                self.object_tool = command;
            }
            chrome::ToolbarCommand::Scale => {
                self.append("Scale is unavailable. A spatial frame stores translation and a quaternion, not scale.");
            }
            chrome::ToolbarCommand::Level => self.enter_mode(chrome::WorkspaceMode::Level, true),
            chrome::ToolbarCommand::Land => self.enter_mode(chrome::WorkspaceMode::Land, true),
            chrome::ToolbarCommand::Character => self.enter_mode(chrome::WorkspaceMode::Character, true),
            chrome::ToolbarCommand::Local => {
                self.transform_space = match self.transform_space {
                    gizmo::TransformSpace::World => gizmo::TransformSpace::Local,
                    gizmo::TransformSpace::Local => gizmo::TransformSpace::World,
                };
                let name = match self.transform_space {
                    gizmo::TransformSpace::World => "world",
                    gizmo::TransformSpace::Local => "local",
                };
                self.append(&format!("Transform space is {name}."));
            }
            chrome::ToolbarCommand::Play => self.begin_play(),
            chrome::ToolbarCommand::Pause => self.pause_play(),
            chrome::ToolbarCommand::Stop => self.stop_play(),
            chrome::ToolbarCommand::Focus => {
                let _ = self.focus_selected();
            }
            chrome::ToolbarCommand::Fly => self.append("Fly is RMB plus WASD. The toolbar icon does not switch a mode."),
            chrome::ToolbarCommand::Pan => self.append("Pan is the middle mouse button."),
            chrome::ToolbarCommand::Orbit => self.append("Orbit is Alt plus the left mouse button."),
            chrome::ToolbarCommand::Snap => {
                if self.land_mode && !self.session_active() {
                    self.land_grid_snap = !self.land_grid_snap;
                    self.append(if self.land_grid_snap {
                        "Terrain snap is on. The brush center locks to the minor grid."
                    } else {
                        "Terrain snap is off."
                    });
                    self.inspector_force_realize = true;
                    self.request_inspector_refresh();
                } else {
                    self.append("Grid snap is the Land Mode brush. Enter Land Mode to snap a sculpt to the terrain grid.");
                }
            }
            chrome::ToolbarCommand::Speed => self.append("Camera speed is the mouse wheel, in meters per second. The toolbar icon does not change it."),
            chrome::ToolbarCommand::Maximize => self.append("Maximize viewport is not implemented."),
        }
        unsafe { InvalidateRect(self.toolbar, std::ptr::null(), 0); }
    }

    fn release_renderer(&mut self) {
        self.end_capture(true, true);
        if !self.ui_font.is_null() {
            unsafe { DeleteObject(self.ui_font as _); }
            self.ui_font = std::ptr::null_mut();
        }
        let Some(renderer) = self.renderer.take() else { return };
        // frame_proc cannot unwind. A device-drop panic would abort the process.
        if let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(renderer))) {
            eprintln!("EDITOR_FRAME renderer release panicked: {}", panic_payload(&payload));
        }
    }
}

fn vec_sub(left: Vec3, right: Vec3) -> Vec3 {
    Vec3::new(left.x - right.x, left.y - right.y, left.z - right.z)
}

fn vec_len(value: Vec3) -> f64 {
    (value.x * value.x + value.y * value.y + value.z * value.z).sqrt()
}

fn rotation_about_x(radians: f64) -> Quat {
    Quat::from_axis_angle(Vec3::new(1.0, 0.0, 0.0), radians).expect("x axis")
}

fn rgb_close(left: [f32; 3], right: [f32; 3]) -> bool {
    left.iter().zip(right).all(|(left, right)| (left - right).abs() < 1.0e-4)
}

fn chroma(color: [f32; 3]) -> [f32; 3] {
    let sum = color[0] + color[1] + color[2];
    if sum.abs() < 1.0e-6 {
        [0.0; 3]
    } else {
        [color[0] / sum, color[1] / sum, color[2] / sum]
    }
}

fn vec_dot(left: Vec3, right: Vec3) -> f64 {
    left.x * right.x + left.y * right.y + left.z * right.z
}

fn place(hwnd: HWND, x: i32, y: i32, width: i32, height: i32) {
    if hwnd.is_null() {
        return;
    }
    unsafe { SetWindowPos(hwnd, std::ptr::null_mut(), x, y, width.max(1), height.max(1), SWP_NOZORDER | SWP_NOACTIVATE); }
}

fn client_size(hwnd: HWND) -> (u32, u32) {
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    unsafe { GetClientRect(hwnd, &mut rect); }
    ((rect.right - rect.left).max(0) as u32, (rect.bottom - rect.top).max(0) as u32)
}

fn dip_rect_px(rect: DipRect, dpi: u32) -> RECT {
    RECT {
        left: dip_to_px(rect.x, dpi),
        top: dip_to_px(rect.y, dpi),
        right: dip_to_px(rect.x + rect.width, dpi),
        bottom: dip_to_px(rect.y + rect.height, dpi),
    }
}

fn point_from_lparam(lparam: LPARAM, dpi: u32) -> DipPoint {
    let x = (lparam & 0xffff) as u16 as i16 as i32;
    let y = ((lparam >> 16) & 0xffff) as u16 as i16 as i32;
    DipPoint { x: px_to_dip(x, dpi), y: px_to_dip(y, dpi) }
}

fn describe(error: RenderError) -> String {
    error.to_string()
}

fn panic_payload(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".into()
    }
}

fn describe_frame(error: FrameError<String>) -> String {
    match error {
        FrameError::Runtime(error) => format!("runtime {error:?}"),
        FrameError::Render(error) => error,
        FrameError::Scene(error) => error.to_string(),
    }
}

fn set_control_text(hwnd: HWND, text: &str) {
    if hwnd.is_null() || window_text(hwnd) == text {
        return;
    }
    let text = wide(text);
    unsafe { SetWindowTextW(hwnd, text.as_ptr()); }
}

fn window_text(hwnd: HWND) -> String {
    unsafe {
        let length = GetWindowTextLengthW(hwnd).max(0) as usize;
        let mut buffer = vec![0u16; length + 1];
        let written = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32).max(0) as usize;
        String::from_utf16_lossy(&buffer[..written])
    }
}

fn key_to_node(key: TreeKey) -> outliner::OutlinerNodeId {
    match key {
        TreeKey::World | TreeKey::Tool(_) => outliner::OutlinerNodeId::WorldRoot,
        TreeKey::Entity(entity) => outliner::OutlinerNodeId::Entity(entity),
    }
}

unsafe fn find_item(hwnd: HWND, parent: HTREEITEM, param: isize) -> Option<HTREEITEM> {
    let mut child = if parent == TVI_ROOT {
        SendMessageW(hwnd, TVM_GETNEXTITEM, TVGN_ROOT as usize, 0)
    } else {
        SendMessageW(hwnd, TVM_GETNEXTITEM, TVGN_CHILD as usize, parent)
    };
    while child != 0 {
        let mut read: TVITEMW = std::mem::zeroed();
        read.mask = TVIF_PARAM;
        read.hItem = child;
        SendMessageW(hwnd, TVM_GETITEMW, 0, &mut read as *mut TVITEMW as LPARAM);
        if read.lParam == param {
            return Some(child);
        }
        if let Some(found) = find_item(hwnd, child, param) {
            return Some(found);
        }
        child = SendMessageW(hwnd, TVM_GETNEXTITEM, TVGN_NEXT as usize, child);
    }
    None
}

unsafe fn outliner_tree(parent: HWND, instance: HINSTANCE) -> Result<HWND, String> {
    let title = wide("World Outliner");
    let hwnd = CreateWindowExW(
        0,
        WC_TREEVIEWW,
        title.as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | TVS_HASBUTTONS | TVS_HASLINES | TVS_LINESATROOT | TVS_SHOWSELALWAYS
            | TVS_FULLROWSELECT
            | TVS_INFOTIP
            | TVS_DISABLEDRAGDROP,
        0,
        0,
        10,
        10,
        parent,
        OUTLINER.raw() as usize as HMENU,
        instance,
        std::ptr::null(),
    );
    if hwnd.is_null() {
        return Err("outliner tree was not created".into());
    }
    SendMessageW(hwnd, TVM_SETBKCOLOR, 0, chrome::PANEL as LPARAM);
    SendMessageW(hwnd, TVM_SETTEXTCOLOR, 0, chrome::TEXT as LPARAM);
    SendMessageW(hwnd, TVM_SETLINECOLOR, 0, chrome::LINE as LPARAM);
    chrome::use_dark_control(hwnd);
    Ok(hwnd)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn axis_matches(components: &[String], axis: u8, value: f64) -> bool {
    components.get(axis as usize).and_then(|text| text.trim().parse::<f64>().ok()).is_some_and(|current| (current - value).abs() < 1.0e-6)
}

fn combo_text(hwnd: HWND) -> String {
    unsafe {
        let index = SendMessageW(hwnd, CB_GETCURSEL, 0, 0);
        if index < 0 {
            return String::new();
        }
        let mut buffer = vec![0u16; 260];
        let copied = SendMessageW(hwnd, CB_GETLBTEXT, index as usize, buffer.as_mut_ptr() as isize);
        if copied <= 0 {
            return String::new();
        }
        let copied = copied as usize;
        String::from_utf16_lossy(&buffer[..copied.min(buffer.len())]).trim_end_matches('\0').to_string()
    }
}

fn choose_linear_color(owner: HWND, linear: Vec3) -> Option<Vec3> {
    use windows_sys::Win32::UI::Controls::Dialogs::{ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW};
    let encode = |value: f64| {
        let value = value.clamp(0.0, 1.0);
        let encoded = if value <= 0.0031308 { value * 12.92 } else { 1.055 * value.powf(1.0 / 2.4) - 0.055 };
        (encoded * 255.0).round().clamp(0.0, 255.0) as u32
    };
    let decode = |value: u32| {
        let encoded = (value as f64 / 255.0).clamp(0.0, 1.0);
        if encoded <= 0.04045 { encoded / 12.92 } else { ((encoded + 0.055) / 1.055).powf(2.4) }
    };
    let mut custom = [0u32; 16];
    let mut dialog: CHOOSECOLORW = unsafe { std::mem::zeroed() };
    dialog.lStructSize = std::mem::size_of::<CHOOSECOLORW>() as u32;
    dialog.hwndOwner = owner;
    dialog.rgbResult = encode(linear.x) | (encode(linear.y) << 8) | (encode(linear.z) << 16);
    dialog.lpCustColors = custom.as_mut_ptr();
    dialog.Flags = CC_RGBINIT | CC_FULLOPEN;
    if unsafe { ChooseColorW(&mut dialog) } == 0 {
        return None;
    }
    let packed = dialog.rgbResult;
    Some(Vec3::new(decode(packed & 0xff), decode((packed >> 8) & 0xff), decode((packed >> 16) & 0xff)))
}

fn judge_rfc0001(rows: &[RfcSample], cache_hit: bool, reference_dirty: bool, uncovered_parents: u32, parent_nodes: u32) -> (bool, String) {
    let mut reasons = Vec::new();
    let Some(sample) = rows.first() else {
        return (false, "[\"no samples\"]".into());
    };
    if sample.root_triangles == 0 {
        reasons.push("\"root triangles are 0\"".to_string());
    }
    if sample.empty_parents > 0 {
        reasons.push(format!("\"empty parents {}\"", sample.empty_parents));
    }
    if parent_nodes > 0 && uncovered_parents.saturating_mul(100) > parent_nodes.saturating_mul(2) {
        reasons.push(format!("\"parents missing child-center coverage {uncovered_parents}/{parent_nodes}\""));
    }
    if !cache_hit {
        reasons.push("\"sidecar was rebuilt; cache-hit launch has not passed\"".into());
    }
    if reference_dirty {
        reasons.push("\"leaf reference was not the full mesh\"".into());
    }
    let at = |threshold: f32, camera: &str| rows.iter().find(|row| (row.threshold - threshold).abs() < 0.05 && row.camera == camera);
    if let Some(close) = at(1.0, "close") {
        if close.leaves < close.parents {
            reasons.push(format!("\"close leaves {} are below parents {}\"", close.leaves, close.parents));
        }
        if close.reduction < -25.0 {
            reasons.push(format!("\"close reduction {:.1}% is past -25%\"", close.reduction));
        }
        push_frame_reasons(&mut reasons, close, 0.35, "close");
    } else {
        reasons.push("\"missing close 1px sample\"".into());
    }
    if let Some(medium) = at(1.0, "medium") {
        if medium.parents == 0 {
            reasons.push("\"medium selected no parents\"".into());
        }
        if medium.reduction > -10.0 {
            reasons.push(format!("\"medium reduction {:.1}% did not fall\"", medium.reduction));
        }
        push_frame_reasons(&mut reasons, medium, 0.10, "medium");
    } else {
        reasons.push("\"missing medium 1px sample\"".into());
    }
    if let Some(far) = at(1.0, "far") {
        if far.parents < far.leaves {
            reasons.push(format!("\"far parents {} are below leaves {}\"", far.parents, far.leaves));
        }
        if far.reduction > -40.0 {
            reasons.push(format!("\"far reduction {:.1}% is weaker than -40%\"", far.reduction));
        }
        push_frame_reasons(&mut reasons, far, 0.05, "far");
    } else {
        reasons.push("\"missing far 1px sample\"".into());
    }
    for row in rows {
        let gated = (row.threshold - 1.0).abs() < 0.05 && matches!(row.camera.as_str(), "close" | "medium" | "far");
        if gated || row.hole_ratio <= 0.015 {
            continue;
        }
        reasons.push(format!("\"{:.1}px {} silhouette holes {:.2}%\"", row.threshold, row.camera, row.hole_ratio * 100.0));
    }
    let passed = reasons.is_empty();
    (passed, format!("[{}]", reasons.join(",")))
}

fn push_frame_reasons(reasons: &mut Vec<String>, row: &RfcSample, minimum: f32, label: &str) {
    if !row.in_frame {
        reasons.push(format!("\"{label} does not contain the whole shop\""));
    }
    let viewport = row.viewport_h.max(1) as f32;
    if row.shop_height_px < viewport * minimum {
        reasons.push(format!("\"{label} shop height {:.0}px is under {:.0}% of the viewport\"", row.shop_height_px, minimum * 100.0));
    }
    if row.hole_ratio > 0.015 {
        reasons.push(format!("\"{label} silhouette holes {:.2}%\"", row.hole_ratio * 100.0));
    }
}

fn project_shop(eye: Vec3, forward: Vec3, right: Vec3, up: Vec3, fov: f64, view_w: u32, view_h: u32, corners: &[Vec3; 8]) -> Option<ShopProjection> {
    let half_y = (fov * 0.5).tan();
    if half_y < 1.0e-4 {
        return None;
    }
    let half_x = half_y * (view_w as f64 / view_h.max(1) as f64);
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    let mut in_frame = true;
    for corner in corners {
        let delta = Vec3::new(corner.x - eye.x, corner.y - eye.y, corner.z - eye.z);
        let depth = delta.x * forward.x + delta.y * forward.y + delta.z * forward.z;
        if depth < 0.05 {
            return None;
        }
        let x = delta.x * right.x + delta.y * right.y + delta.z * right.z;
        let y = delta.x * up.x + delta.y * up.y + delta.z * up.z;
        let ndc_x = x / (depth * half_x);
        let ndc_y = y / (depth * half_y);
        if ndc_x.abs() > 0.92 || ndc_y.abs() > 0.92 {
            in_frame = false;
        }
        let px = (ndc_x * 0.5 + 0.5) * view_w as f64;
        let py = (1.0 - (ndc_y * 0.5 + 0.5)) * view_h as f64;
        min_x = min_x.min(px);
        min_y = min_y.min(py);
        max_x = max_x.max(px);
        max_y = max_y.max(py);
    }
    Some(ShopProjection { min_x, min_y, max_x, max_y, height: (max_y - min_y).max(0.0), in_frame })
}

fn shop_region(projection: Option<&ShopProjection>, origin_x: i32, origin_y: i32, view_w: i32, view_h: i32, image_w: i32, image_h: i32) -> (i32, i32, i32, i32) {
    let (x0, y0, x1, y1) = if let Some(projection) = projection {
        (
            origin_x + projection.min_x.floor() as i32 - 4,
            origin_y + projection.min_y.floor() as i32 - 4,
            origin_x + projection.max_x.ceil() as i32 + 4,
            origin_y + projection.max_y.ceil() as i32 + 4,
        )
    } else {
        (origin_x, origin_y, origin_x + view_w, origin_y + view_h)
    };
    let left = x0.clamp(0, image_w.saturating_sub(1));
    let top = y0.clamp(0, image_h.saturating_sub(1));
    let right = x1.clamp(left + 1, image_w);
    let bottom = y1.clamp(top + 1, image_h);
    (left, top, right - left, bottom - top)
}

fn sky_color(rgba: &[u8], width: i32, origin_x: i32, origin_y: i32, view_w: i32, _view_h: i32) -> [u8; 3] {
    let mut bins = vec![0u32; 4096];
    let mut best = [96u8, 128, 160];
    let mut best_count = 0u32;
    let sample = |x: i32, y: i32, bins: &mut [u32], best: &mut [u8; 3], best_count: &mut u32| {
        if x < 0 || y < 0 || x >= width {
            return;
        }
        let index = ((y as usize) * (width as usize) + x as usize) * 4;
        let Some(pixel) = rgba.get(index..index + 3) else { return };
        let key = ((pixel[0] as usize) >> 4) << 8 | ((pixel[1] as usize) >> 4) << 4 | ((pixel[2] as usize) >> 4);
        bins[key] = bins[key].saturating_add(1);
        if bins[key] > *best_count {
            *best_count = bins[key];
            *best = [pixel[0], pixel[1], pixel[2]];
        }
    };
    let y = origin_y.clamp(0, i32::MAX);
    let step = (view_w / 48).max(1);
    let mut x = origin_x;
    while x < origin_x + view_w {
        sample(x, y + 2, &mut bins, &mut best, &mut best_count);
        x += step;
    }
    best
}

struct ReliefMetrics {
    mean_diff: f32,
    changed_fraction: f32,
    marks: u32,
    overlap: f32,
    outside_diff: f32,
    holes: u32,
    solid: u32,
    hole_ratio: f32,
}

fn channel_delta(left: &[u8], right: &[u8]) -> u16 {
    u16::from((left[0] as i16 - right[0] as i16).unsigned_abs())
        + u16::from((left[1] as i16 - right[1] as i16).unsigned_abs())
        + u16::from((left[2] as i16 - right[2] as i16).unsigned_abs())
}

fn is_micro_mark(pixel: &[u8]) -> bool {
    pixel[0] > 160 && pixel[2] < 150 && u16::from(pixel[0]) + 30 > u16::from(pixel[1]) && u16::from(pixel[0]) > u16::from(pixel[2]) + 25
}

fn relief_metrics(off: &ReliefShot, on: &ReliefShot, color: &ReliefShot) -> ReliefMetrics {
    let empty = ReliefMetrics { mean_diff: 255.0, changed_fraction: 1.0, marks: 0, overlap: 0.0, outside_diff: 255.0, holes: 1, solid: 1, hole_ratio: 1.0 };
    let (width, height, plain) = crop_rgba(&off.rgba, off.width, off.region);
    let (_, _, shaded_raw) = crop_rgba(&on.rgba, on.width, on.region);
    let (_, _, marked_raw) = crop_rgba(&color.rgba, color.width, color.region);
    let (_, _, shaded) = scale_nearest(&shaded_raw, on.region.2, on.region.3, width, height);
    let (_, _, marked) = scale_nearest(&marked_raw, color.region.2, color.region.3, width, height);
    if plain.len() != shaded.len() || plain.len() != marked.len() || width <= 1 || height <= 1 {
        return empty;
    }
    let sky = sky_color(&off.rgba, off.width, off.origin.0, off.origin.1, off.origin.2, off.origin.3);
    let mut diff_sum = 0u64;
    let mut changed = 0u32;
    let mut total = 0u32;
    let mut marks = 0u32;
    let mut mark_hit = 0u32;
    let mut outside_sum = 0u64;
    let mut outside_count = 0u32;
    let mut plain_solid = 0u32;
    let mut shaded_solid = 0u32;
    for index in (0..plain.len()).step_by(4) {
        let delta = channel_delta(&plain[index..index + 3], &shaded[index..index + 3]);
        diff_sum += u64::from(delta);
        total = total.saturating_add(1);
        if delta > 24 {
            changed = changed.saturating_add(1);
        }
        if is_micro_mark(&marked[index..index + 3]) {
            marks = marks.saturating_add(1);
            if delta > 12 {
                mark_hit = mark_hit.saturating_add(1);
            }
        } else {
            outside_sum += u64::from(delta);
            outside_count = outside_count.saturating_add(1);
        }
        if !near_sky(&plain[index..index + 3], sky) {
            plain_solid = plain_solid.saturating_add(1);
        }
        if !near_sky(&shaded[index..index + 3], sky) {
            shaded_solid = shaded_solid.saturating_add(1);
        }
    }
    let hole_gap = plain_solid.abs_diff(shaded_solid);
    ReliefMetrics {
        mean_diff: if total == 0 { 255.0 } else { diff_sum as f32 / total as f32 / 3.0 },
        changed_fraction: if total == 0 { 1.0 } else { changed as f32 / total as f32 },
        marks,
        overlap: if marks == 0 { 0.0 } else { mark_hit as f32 / marks as f32 },
        outside_diff: if outside_count == 0 { 0.0 } else { outside_sum as f32 / outside_count as f32 / 3.0 },
        holes: hole_gap,
        solid: plain_solid,
        hole_ratio: if plain_solid < 64 { 1.0 } else { hole_gap as f32 / plain_solid as f32 },
    }
}

fn near_sky(pixel: &[u8], sky: [u8; 3]) -> bool {
    (pixel[0] as i32 - sky[0] as i32).abs() + (pixel[1] as i32 - sky[1] as i32).abs() + (pixel[2] as i32 - sky[2] as i32).abs() <= 42
}

fn crop_rgba(rgba: &[u8], width: i32, region: (i32, i32, i32, i32)) -> (i32, i32, Vec<u8>) {
    let (left, top, region_w, region_h) = region;
    if region_w <= 0 || region_h <= 0 || width <= 0 {
        return (1, 1, vec![0, 0, 0, 255]);
    }
    let mut out = vec![0u8; (region_w as usize) * (region_h as usize) * 4];
    for y in 0..region_h {
        for x in 0..region_w {
            let src = (((top + y) as usize) * (width as usize) + (left + x) as usize) * 4;
            let dst = (y as usize * region_w as usize + x as usize) * 4;
            if let Some(pixel) = rgba.get(src..src + 4) {
                out[dst..dst + 4].copy_from_slice(pixel);
            }
        }
    }
    (region_w, region_h, out)
}

fn scale_nearest(rgba: &[u8], width: i32, height: i32, target_w: i32, target_h: i32) -> (i32, i32, Vec<u8>) {
    if width <= 0 || height <= 0 || target_w <= 0 || target_h <= 0 {
        return (1, 1, vec![0, 0, 0, 255]);
    }
    let mut out = vec![0u8; (target_w as usize) * (target_h as usize) * 4];
    for y in 0..target_h {
        let src_y = (y as i64 * height as i64 / target_h as i64).min(height as i64 - 1) as i32;
        for x in 0..target_w {
            let src_x = (x as i64 * width as i64 / target_w as i64).min(width as i64 - 1) as i32;
            let src = (src_y as usize * width as usize + src_x as usize) * 4;
            let dst = (y as usize * target_w as usize + x as usize) * 4;
            if let Some(pixel) = rgba.get(src..src + 4) {
                out[dst..dst + 4].copy_from_slice(pixel);
            }
        }
    }
    (target_w, target_h, out)
}

fn compose_shots(shots: &[&ReliefShot]) -> Option<CapturedWindow> {
    let first = *shots.first()?;
    let (crop_w, crop_h, _) = crop_rgba(&first.rgba, first.width, first.region);
    if crop_w <= 1 || crop_h <= 1 || shots.is_empty() {
        return None;
    }
    let panels: Vec<Vec<u8>> = shots
        .iter()
        .map(|shot| {
            let (_, _, raw) = crop_rgba(&shot.rgba, shot.width, shot.region);
            let (_, _, scaled) = scale_nearest(&raw, shot.region.2, shot.region.3, crop_w, crop_h);
            scaled
        })
        .collect();
    let width = crop_w.saturating_mul(panels.len() as i32);
    let mut rgba = vec![0u8; (width as usize) * (crop_h as usize) * 4];
    for (panel_index, panel) in panels.iter().enumerate() {
        for y in 0..crop_h {
            for x in 0..crop_w {
                let src = (y as usize * crop_w as usize + x as usize) * 4;
                let dst = (y as usize * width as usize + (panel_index * crop_w as usize + x as usize)) * 4;
                if src + 4 <= panel.len() && dst + 4 <= rgba.len() {
                    rgba[dst..dst + 4].copy_from_slice(&panel[src..src + 4]);
                }
            }
        }
    }
    Some(CapturedWindow { width, height: crop_h, rgba })
}

fn compose_review(off: &ReliefShot, on: &ReliefShot, color: &ReliefShot) -> Option<CapturedWindow> {
    let (crop_w, crop_h, plain) = crop_rgba(&off.rgba, off.width, off.region);
    let (_, _, shaded_raw) = crop_rgba(&on.rgba, on.width, on.region);
    let (_, _, marked_raw) = crop_rgba(&color.rgba, color.width, color.region);
    let (_, _, shaded) = scale_nearest(&shaded_raw, on.region.2, on.region.3, crop_w, crop_h);
    let (_, _, marked) = scale_nearest(&marked_raw, color.region.2, color.region.3, crop_w, crop_h);
    if plain.len() != shaded.len() || plain.len() != marked.len() || crop_w <= 1 {
        return None;
    }
    let mut diff = vec![0u8; plain.len()];
    for index in (0..plain.len().min(shaded.len())).step_by(4) {
        for channel in 0..3 {
            let delta = u16::from((plain[index + channel] as i16 - shaded[index + channel] as i16).unsigned_abs()).saturating_mul(8);
            diff[index + channel] = u8::try_from(delta.min(255)).unwrap_or(255);
        }
        diff[index + 3] = 255;
    }
    let panels = [&plain, &shaded, &marked, &diff];
    let width = crop_w.saturating_mul(panels.len() as i32);
    let mut rgba = vec![0u8; (width as usize) * (crop_h as usize) * 4];
    for (panel_index, panel) in panels.iter().enumerate() {
        for y in 0..crop_h {
            for x in 0..crop_w {
                let src = (y as usize * crop_w as usize + x as usize) * 4;
                let dst = (y as usize * width as usize + (panel_index * crop_w as usize + x as usize)) * 4;
                if src + 4 <= panel.len() && dst + 4 <= rgba.len() {
                    rgba[dst..dst + 4].copy_from_slice(&panel[src..src + 4]);
                }
            }
        }
    }
    Some(CapturedWindow { width, height: crop_h, rgba })
}

fn compose_transition_strip(crops: &[(f32, Vec<u8>, i32, i32)]) -> Option<CapturedWindow> {
    if crops.is_empty() {
        return None;
    }
    let height = crops.iter().map(|crop| crop.3).max().unwrap_or(0);
    let width = crops.iter().map(|crop| crop.2).sum::<i32>();
    if width <= 0 || height <= 0 {
        return None;
    }
    let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
    let mut x_offset = 0i32;
    for (_, pixels, crop_w, crop_h) in crops {
        for y in 0..*crop_h {
            for x in 0..*crop_w {
                let src = (y as usize * *crop_w as usize + x as usize) * 4;
                let dst = (y as usize * width as usize + (x_offset + x) as usize) * 4;
                if src + 4 <= pixels.len() && dst + 4 <= rgba.len() {
                    rgba[dst..dst + 4].copy_from_slice(&pixels[src..src + 4]);
                }
            }
        }
        x_offset += crop_w;
    }
    Some(CapturedWindow { width, height, rgba })
}

fn grouped(value: u32) -> String {
    let text = value.to_string();
    let mut out = String::new();
    for (index, ch) in text.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn orange_fraction(rgba: &[u8], width: i32, region: (i32, i32, i32, i32)) -> f32 {
    let (left, top, region_w, region_h) = region;
    if region_w <= 0 || region_h <= 0 || width <= 0 {
        return 0.0;
    }
    let mut orange = 0u32;
    let mut total = 0u32;
    for y in top..top + region_h {
        for x in left..left + region_w {
            let index = ((y as usize) * (width as usize) + x as usize) * 4;
            let Some(pixel) = rgba.get(index..index + 3) else { continue };
            total = total.saturating_add(1);
            if pixel[0] > 150 && u16::from(pixel[0]) > u16::from(pixel[1]) + 20 && u16::from(pixel[0]) > u16::from(pixel[2]) + 40 {
                orange = orange.saturating_add(1);
            }
        }
    }
    if total == 0 { 0.0 } else { orange as f32 / total as f32 }
}

fn saturated_fraction(rgba: &[u8], width: i32, region: (i32, i32, i32, i32)) -> f32 {
    let (left, top, region_w, region_h) = region;
    if region_w <= 0 || region_h <= 0 || width <= 0 {
        return 0.0;
    }
    let mut saturated = 0u32;
    let mut total = 0u32;
    for y in top..top + region_h {
        for x in left..left + region_w {
            let index = ((y as usize) * (width as usize) + x as usize) * 4;
            let Some(pixel) = rgba.get(index..index + 3) else { continue };
            let max = pixel[0].max(pixel[1]).max(pixel[2]);
            let min = pixel[0].min(pixel[1]).min(pixel[2]);
            total = total.saturating_add(1);
            if u16::from(max) - u16::from(min) > 70 {
                saturated = saturated.saturating_add(1);
            }
        }
    }
    if total == 0 { 0.0 } else { saturated as f32 / total as f32 }
}

fn mean_abs_diff(left: &[u8], right: &[u8], width: i32, left_region: (i32, i32, i32, i32), right_region: (i32, i32, i32, i32)) -> f32 {
    if left_region != right_region || width <= 0 {
        return 255.0;
    }
    let (x0, y0, region_w, region_h) = left_region;
    let mut sum = 0u64;
    let mut count = 0u64;
    for y in y0..y0 + region_h {
        for x in x0..x0 + region_w {
            let index = ((y as usize) * (width as usize) + x as usize) * 4;
            let (Some(a), Some(b)) = (left.get(index..index + 3), right.get(index..index + 3)) else { continue };
            sum += (a[0] as i16 - b[0] as i16).unsigned_abs() as u64;
            sum += (a[1] as i16 - b[1] as i16).unsigned_abs() as u64;
            sum += (a[2] as i16 - b[2] as i16).unsigned_abs() as u64;
            count += 3;
        }
    }
    if count == 0 { 255.0 } else { sum as f32 / count as f32 }
}

fn count_silhouette_holes(reference: &[u8], candidate: &[u8], width: i32, region: (i32, i32, i32, i32), sky: [u8; 3], slack: i32) -> (u32, u32) {
    let (left, top, region_w, region_h) = region;
    if region_w <= 0 || region_h <= 0 || width <= 0 {
        return (0, 0);
    }
    let occupied = |image: &[u8], x: i32, y: i32| -> bool {
        let index = ((y as usize) * (width as usize) + x as usize) * 4;
        let Some(pixel) = image.get(index..index + 3) else { return false };
        let distance = (pixel[0] as i32 - sky[0] as i32).abs() + (pixel[1] as i32 - sky[1] as i32).abs() + (pixel[2] as i32 - sky[2] as i32).abs();
        distance > 42
    };
    let mut reference_mask = vec![false; (region_w as usize) * (region_h as usize)];
    let mut candidate_mask = vec![false; reference_mask.len()];
    for y in 0..region_h {
        for x in 0..region_w {
            let slot = (y as usize) * (region_w as usize) + x as usize;
            reference_mask[slot] = occupied(reference, left + x, top + y);
            candidate_mask[slot] = occupied(candidate, left + x, top + y);
        }
    }
    let slack = slack.clamp(0, 8);
    let mut dilated = candidate_mask.clone();
    if slack > 0 {
        for y in 0..region_h {
            for x in 0..region_w {
                let slot = (y as usize) * (region_w as usize) + x as usize;
                if !candidate_mask[slot] {
                    continue;
                }
                let y0 = (y - slack).max(0);
                let y1 = (y + slack).min(region_h - 1);
                let x0 = (x - slack).max(0);
                let x1 = (x + slack).min(region_w - 1);
                for ny in y0..=y1 {
                    for nx in x0..=x1 {
                        dilated[(ny as usize) * (region_w as usize) + nx as usize] = true;
                    }
                }
            }
        }
    }
    let mut holes = 0u32;
    let mut solid = 0u32;
    for (reference_pixel, covered) in reference_mask.iter().zip(dilated.iter()) {
        if *reference_pixel {
            solid = solid.saturating_add(1);
            if !covered {
                holes = holes.saturating_add(1);
            }
        }
    }
    (holes, solid)
}

fn viewport_origin(frame: HWND, viewport: HWND) -> (i32, i32, i32, i32) {
    unsafe {
        let mut view: RECT = std::mem::zeroed();
        let mut window: RECT = std::mem::zeroed();
        if viewport.is_null() || GetWindowRect(viewport, &mut view) == 0 || GetWindowRect(frame, &mut window) == 0 {
            return (0, 0, 1, 1);
        }
        (view.left - window.left, view.top - window.top, (view.right - view.left).max(1), (view.bottom - view.top).max(1))
    }
}

fn row_under_root(outline: &[jarvig_core::EntityOutlineInfo], uuid: jarvig_core::EntityId, root_name: &str) -> bool {
    let mut cursor = Some(uuid);
    let mut guard = 0;
    while let Some(id) = cursor {
        guard += 1;
        if guard > 64 {
            return false;
        }
        let Some(row) = outline.iter().find(|row| row.uuid == id) else { return false };
        if row.parent.is_none() {
            return row.name == root_name;
        }
        cursor = row.parent;
    }
    false
}

fn bind_shot(index: u8) -> (&'static str, &'static str, f64) {
    match index {
        0 => ("Base", "front", 0.0),
        1 => ("Base", "side", -std::f64::consts::FRAC_PI_2),
        2 => ("Base", "three-quarter", -0.6),
        3 => ("Base Male", "front", 0.0),
        4 => ("Base Male", "side", -std::f64::consts::FRAC_PI_2),
        _ => ("Base Male", "three-quarter", -0.6),
    }
}

fn capture_window(hwnd: HWND, path: &str) -> Result<(), String> {
    let image = capture_window_image(hwnd)?;
    write_bmp(path, &image)
}

fn capture_window_image(hwnd: HWND) -> Result<CapturedWindow, String> {
    unsafe {
        UpdateWindow(hwnd);
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return Err("window rect".into());
        }
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width <= 0 || height <= 0 {
            return Err("empty client".into());
        }
        let screen = GetDC(hwnd);
        if screen.is_null() {
            return Err("window dc".into());
        }
        let memory = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        if memory.is_null() || bitmap.is_null() {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            if !memory.is_null() {
                DeleteDC(memory);
            }
            ReleaseDC(hwnd, screen);
            return Err("bitmap".into());
        }
        let previous = SelectObject(memory, bitmap);
        let printed = PrintWindow(hwnd, memory, PW_RENDERFULLCONTENT);
        if printed == 0 {
            BitBlt(memory, 0, 0, width, height, screen, 0, 0, SRCCOPY);
        }
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: (width as u32) * (height as u32) * 4,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [windows_sys::Win32::Graphics::Gdi::RGBQUAD {
                rgbBlue: 0,
                rgbGreen: 0,
                rgbRed: 0,
                rgbReserved: 0,
            }],
        };
        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        let rows = GetDIBits(memory, bitmap, 0, height as u32, pixels.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS);
        SelectObject(memory, previous);
        DeleteObject(bitmap);
        DeleteDC(memory);
        ReleaseDC(hwnd, screen);
        if rows == 0 {
            return Err("GetDIBits".into());
        }
        let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
        for y in 0..height {
            let source_y = height - 1 - y;
            for x in 0..width {
                let source = ((source_y as usize) * (width as usize) + x as usize) * 4;
                let dest = ((y as usize) * (width as usize) + x as usize) * 4;
                rgba[dest] = pixels[source + 2];
                rgba[dest + 1] = pixels[source + 1];
                rgba[dest + 2] = pixels[source];
                rgba[dest + 3] = 255;
            }
        }
        Ok(CapturedWindow { width, height, rgba })
    }
}

fn write_bmp(path: &str, image: &CapturedWindow) -> Result<(), String> {
    let width = image.width;
    let height = image.height;
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    for y in 0..height {
        let source_y = height - 1 - y;
        for x in 0..width {
            let source = ((source_y as usize) * (width as usize) + x as usize) * 4;
            let dest = ((y as usize) * (width as usize) + x as usize) * 4;
            pixels[dest] = image.rgba[source + 2];
            pixels[dest + 1] = image.rgba[source + 1];
            pixels[dest + 2] = image.rgba[source];
            pixels[dest + 3] = 255;
        }
    }
    let mut bytes = Vec::with_capacity(54 + pixels.len());
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&(54u32 + pixels.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0i32.to_le_bytes());
    bytes.extend_from_slice(&0i32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&pixels);
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}

fn write_png(path: &str, image: &CapturedWindow) -> Result<(), String> {
    if image.width <= 0 || image.height <= 0 {
        return Err("empty image".into());
    }
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let file = std::fs::File::create(path).map_err(|error| error.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.width as u32, image.height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    writer.write_image_data(&image.rgba).map_err(|error| error.to_string())
}

#[cfg(test)]
mod rfc0001_harness {
    use super::count_silhouette_holes;

    fn fill(width: usize, height: usize, color: [u8; 3]) -> Vec<u8> {
        let mut pixels = vec![255u8; width * height * 4];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[0] = color[0];
            pixel[1] = color[1];
            pixel[2] = color[2];
        }
        pixels
    }

    #[test]
    fn a_missing_block_is_a_hole_and_a_two_pixel_shift_is_not() {
        let sky = [70, 110, 150];
        let mut reference = fill(16, 16, sky);
        let mut shifted = reference.clone();
        let mut opened = reference.clone();
        for y in 4..12 {
            for x in 4..12 {
                let index = (y * 16 + x) * 4;
                reference[index] = 180;
                reference[index + 1] = 140;
                reference[index + 2] = 90;
                shifted[index] = 180;
                shifted[index + 1] = 140;
                shifted[index + 2] = 90;
            }
        }
        for y in 4..12 {
            for x in 6..14 {
                let index = (y * 16 + x) * 4;
                shifted[index] = 180;
                shifted[index + 1] = 140;
                shifted[index + 2] = 90;
            }
        }
        for y in 5..11 {
            for x in 5..11 {
                let index = (y * 16 + x) * 4;
                opened[index] = sky[0];
                opened[index + 1] = sky[1];
                opened[index + 2] = sky[2];
            }
        }
        let region = (0, 0, 16, 16);
        let (shift_holes, solid) = count_silhouette_holes(&reference, &shifted, 16, region, sky, 2);
        let (open_holes, _) = count_silhouette_holes(&reference, &opened, 16, region, sky, 2);
        assert!(solid >= 64);
        assert_eq!(shift_holes, 0);
        assert!(open_holes > 0);
    }
}

unsafe fn register_classes(instance: HINSTANCE) -> Result<(), String> {
    let frame = wide(CLASS_FRAME);
    let toolbar = wide(CLASS_TOOLBAR);
    let dock = wide(CLASS_DOCK);
    let viewport = wide(CLASS_VIEWPORT);
    let drop = wide(CLASS_DROP);
    let inspector = wide(CLASS_INSPECTOR);
    let load = wide(CLASS_LOAD);
    let status = wide(CLASS_STATUS);
    let frame_class = WNDCLASSW {
        lpfnWndProc: Some(frame_proc),
        hInstance: instance,
        hIcon: LoadIconW(std::ptr::null_mut(), IDI_APPLICATION),
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        hbrBackground: chrome::chrome_brush(),
        lpszClassName: frame.as_ptr(),
        ..std::mem::zeroed()
    };
    if RegisterClassW(&frame_class) == 0 {
        return Err("editor frame class was not registered".into());
    }
    let toolbar_class = WNDCLASSW {
        lpfnWndProc: Some(toolbar_proc),
        hInstance: instance,
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        hbrBackground: chrome::chrome_brush(),
        lpszClassName: toolbar.as_ptr(),
        ..std::mem::zeroed()
    };
    if RegisterClassW(&toolbar_class) == 0 {
        return Err("toolbar class was not registered".into());
    }
    let dock_class = WNDCLASSW {
        lpfnWndProc: Some(dock_proc),
        hInstance: instance,
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        hbrBackground: chrome::panel_brush(),
        lpszClassName: dock.as_ptr(),
        ..std::mem::zeroed()
    };
    if RegisterClassW(&dock_class) == 0 {
        return Err("dock class was not registered".into());
    }
    let viewport_class = WNDCLASSW {
        lpfnWndProc: Some(viewport_proc),
        hInstance: instance,
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        lpszClassName: viewport.as_ptr(),
        ..std::mem::zeroed()
    };
    if RegisterClassW(&viewport_class) == 0 {
        return Err("viewport class was not registered".into());
    }
    let drop_class = WNDCLASSW {
        lpfnWndProc: Some(drop_proc),
        hInstance: instance,
        hbrBackground: chrome::chrome_brush(),
        lpszClassName: drop.as_ptr(),
        ..std::mem::zeroed()
    };
    if RegisterClassW(&drop_class) == 0 {
        return Err("drop class was not registered".into());
    }
    let content_name = wide(CLASS_CONTENT);
    let content_class = WNDCLASSW {
        lpfnWndProc: Some(content_proc),
        hInstance: instance,
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        hbrBackground: chrome::panel_brush(),
        lpszClassName: content_name.as_ptr(),
        ..std::mem::zeroed()
    };
    if RegisterClassW(&content_class) == 0 {
        return Err("content browser class was not registered".into());
    }
    let inspector_class = WNDCLASSW {
        lpfnWndProc: Some(inspector_proc),
        hInstance: instance,
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        hbrBackground: chrome::panel_brush(),
        lpszClassName: inspector.as_ptr(),
        ..std::mem::zeroed()
    };
    if RegisterClassW(&inspector_class) == 0 {
        return Err("inspector class was not registered".into());
    }
    let load_class = WNDCLASSW {
        lpfnWndProc: Some(load_overlay_proc),
        hInstance: instance,
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        lpszClassName: load.as_ptr(),
        ..std::mem::zeroed()
    };
    if RegisterClassW(&load_class) == 0 {
        return Err("load overlay class was not registered".into());
    }
    let status_class = WNDCLASSW {
        lpfnWndProc: Some(status_proc),
        hInstance: instance,
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        lpszClassName: status.as_ptr(),
        ..std::mem::zeroed()
    };
    if RegisterClassW(&status_class) == 0 {
        return Err("status class was not registered".into());
    }
    Ok(())
}

unsafe fn editor_menu() -> Result<(HMENU, HMENU, HMENU, HMENU, HMENU), String> {
    let menu = CreateMenu();
    let file = CreatePopupMenu();
    let edit = CreatePopupMenu();
    let view = CreatePopupMenu();
    let lighting = CreatePopupMenu();
    let build = CreatePopupMenu();
    let play = CreatePopupMenu();
    let help = CreatePopupMenu();
    if menu.is_null()
        || file.is_null()
        || edit.is_null()
        || view.is_null()
        || lighting.is_null()
        || build.is_null()
        || play.is_null()
        || help.is_null()
    {
        return Err("editor menu was not created".into());
    }
    let recent_projects = CreatePopupMenu();
    let recent_levels = CreatePopupMenu();
    let templates = CreatePopupMenu();
    if templates.is_null() {
        return Err("project template menu was not created".into());
    }
    append(templates, ID_FILE_TEMPLATE_EMPTY, "Empty World");
    append(templates, ID_FILE_TEMPLATE_TERRAIN, "Terrain World");
    append(templates, ID_FILE_TEMPLATE_THIRD, "Third Person");
    append(templates, ID_FILE_TEMPLATE_FPS, "FPS");
    popup(file, templates, "New Project");
    append(file, ID_FILE_OPEN_PROJECT, "Open Project...");
    append(file, ID_FILE_CLOSE_PROJECT, "Close Project");
    separator(file);
    append(file, ID_FILE_NEW_LEVEL, "New Level...");
    append(file, ID_FILE_OPEN_LEVEL, "Open Level...");
    append(file, ID_FILE_RELOAD, "Reload Level");
    append(file, ID_FILE_SAVE, "Save");
    append(file, ID_FILE_SAVE_AS, "Save As...");
    append(file, ID_FILE_SAVE_ALL, "Save All");
    append(file, ID_FILE_IMPORT_MESH, "Import Mesh...");
    separator(file);
    popup(file, recent_projects, "Recent Projects");
    popup(file, recent_levels, "Recent Levels");
    separator(file);
    append(file, ID_FILE_EXIT, "Exit");
    append(edit, ID_EDIT_UNDO, "Undo Mesh Placement\tCtrl+Z");
    append(view, ID_VIEW_OUTLINER, "World Outliner");
    append(view, ID_VIEW_CHARACTER, "Character Editor");
    append(view, ID_VIEW_LAND, "Land Mode");
    append(view, ID_VIEW_RESET_POSE, "Reset Pose");
    append(view, ID_VIEW_JOINTS, "Show Joints");
    append(view, ID_VIEW_ALL_JOINTS, "Show All Joints");
    append(view, ID_VIEW_JOINT_LIMITS, "Show Joint Limits");
    append(view, ID_VIEW_INSPECTOR, "Inspector");
    append(view, ID_VIEW_CONTENT, "Content Browser");
    append(view, ID_VIEW_OUTPUT, "Output Log");
    append(view, ID_VIEW_RESET, "Reset Layout");
    append(view, ID_VIEW_EXPOSURE_UP, "Exposure +");
    append(view, ID_VIEW_EXPOSURE_DOWN, "Exposure -");
    append(view, ID_VIEW_EXPOSURE_RESET, "Reset Exposure");
    append(view, ID_QUALITY_BASELINE, "Renderer Quality: Baseline");
    append(view, ID_QUALITY_ENHANCED, "Renderer Quality: Enhanced");
    append(view, ID_QUALITY_HIGH, "Renderer Quality: High");
    append(view, ID_PRESENT_DITHER, "Presentation Dither");
    append(view, ID_PRESENT_TONEMAP, "Presentation: Tone Map");
    append(view, ID_PRESENT_QUANTIZED, "Presentation: 8-bit Steps");
    append(view, ID_PRESENT_BEFORE, "Presentation: Before Tone Curve");
    separator(view);
    append(view, ID_VIEW_CREATE_CAMERA, "Create Camera Actor");
    append(view, ID_VIEW_STARTUP_CAMERA, "Set Selected as Startup Camera");
    append(view, ID_VIEW_PILOT, "Pilot Selected Camera");
    append(view, ID_VIEW_MESHLETS, "Show Meshlet Colors");
    append(view, ID_VIEW_MESHLET_SHADE, "Draw From Meshlets");
    append(view, ID_VIEW_MESHLET_FRUSTUM, "Frustum Cull Meshlets");
    append(view, ID_VIEW_MESHLET_OCCLUSION, "Occlusion Cull Meshlets");
    append(view, ID_VIEW_MESHLET_FREEZE, "Freeze Meshlet Visibility");
    append(view, ID_VIEW_MESHLET_HIGHLIGHT, "Highlight Cluster Under Cursor");
    append(view, ID_VIEW_CLUSTER_HIERARCHY, "Cluster Hierarchy");
    append(view, ID_VIEW_EINSTEIN, "Einstein Detail Debug");
    append(view, ID_VIEW_MICRO, "Procedural Microgeometry");
    append(view, ID_VIEW_MICRO_COLOR, "Microtriangle Colors");
    append(view, ID_VIEW_ERROR_HALF, "Hierarchy Error 0.5 px");
    append(view, ID_VIEW_ERROR_ONE, "Hierarchy Error 1 px");
    append(view, ID_VIEW_ERROR_TWO, "Hierarchy Error 2 px");
    append(view, ID_VIEW_ERROR_FOUR, "Hierarchy Error 4 px");
    append(view, ID_VIEW_BACKGROUND_JOBS, "Background Jobs");
    append(view, ID_VIEW_CANCEL_JOB, "Cancel Background Job");
    append(view, ID_VIEW_ENVIRONMENT, "Environment Light");
    append(view, ID_VIEW_RECAPTURE, "Recapture Reflection Probes");
    append(view, ID_PROBE_STATIC, "Probe Update: Static");
    append(view, ID_PROBE_ON_DEMAND, "Probe Update: On Demand");
    append(view, ID_PROBE_ON_TRANSFORM, "Probe Update: On Transform");
    append(view, ID_PROBE_ON_LIGHTING, "Probe Update: On Lighting");
    append(view, ID_PROBE_TIME_SLICED, "Probe Update: Time Sliced");
    append(view, ID_PROBE_RES_32, "Probe Resolution 32");
    append(view, ID_PROBE_RES_64, "Probe Resolution 64");
    append(view, ID_PROBE_RES_128, "Probe Resolution 128");
    append(view, ID_PROBE_RES_256, "Probe Resolution 256");
    append(lighting, ID_DEBUG_FULL, "Full Lighting");
    append(lighting, ID_DEBUG_DIRECT, "Direct Only");
    append(lighting, ID_DEBUG_ENV_DIFFUSE, "Environment Diffuse Only");
    append(lighting, ID_DEBUG_ENV_SPECULAR, "Environment Specular Only");
    append(lighting, ID_DEBUG_PROBE_ONLY, "Local Probe Specular Only");
    append(lighting, ID_DEBUG_EMISSIVE, "Emissive Only");
    separator(lighting);
    append(lighting, ID_DEBUG_DIRECTIONAL, "Directional Light");
    append(lighting, ID_DEBUG_POINT, "Point Light");
    append(lighting, ID_DEBUG_SPOT, "Spot Light");
    separator(lighting);
    append(lighting, ID_DEBUG_GLOBAL_ENV, "Global Environment");
    append(lighting, ID_DEBUG_PROBE, "Reflection Probe");
    separator(lighting);
    append(lighting, ID_DEBUG_DIRECTIONAL_ONLY, "Directional Only");
    append(lighting, ID_DEBUG_POINT_ONLY, "Point Only");
    append(lighting, ID_DEBUG_SPOT_ONLY, "Spot Only");
    append(lighting, ID_DEBUG_NO_SHADOWS, "No Shadows");
    append(lighting, ID_DEBUG_INDIRECT_ONLY, "Indirect Diffuse Only");
    append(lighting, ID_DEBUG_DIRECT_UNSHADOWED, "Direct Unshadowed");
    separator(lighting);
    append(lighting, ID_DEBUG_CASCADES, "Shadow Cascades");
    append(lighting, ID_DEBUG_CONTACT, "Contact Shadows");
    separator(lighting);
    append(lighting, ID_MAT_FULL, "Material: Full");
    append(lighting, ID_MAT_BASE, "Material: Base Color");
    append(lighting, ID_MAT_NORMAL, "Material: Normal");
    append(lighting, ID_MAT_ROUGH, "Material: Roughness");
    append(lighting, ID_MAT_AO, "Material: AO");
    append(lighting, ID_MAT_METAL, "Material: Metallic");
    popup(view, lighting, "Lighting Debug");
    append(play, ID_PLAY, "Play In Editor");
    append(play, ID_PLAY_STANDALONE, "Run Standalone");
    append(build, ID_BUILD_PROJECT, "Build Project");
    append(build, ID_BUILD_AND_RUN, "Build & Run");
    append(build, ID_BUILD_SETTINGS, "Build Settings");
    append(help, ID_HELP_ABOUT, "About JARVIGEditor");
    popup(menu, file, "File");
    popup(menu, edit, "Edit");
    popup(menu, view, "View");
    popup(menu, build, "Build");
    popup(menu, play, "Play");
    popup(menu, help, "Help");
    chrome::darken_menu(menu);
    chrome::darken_menu(file);
    chrome::darken_menu(templates);
    chrome::darken_menu(edit);
    chrome::darken_menu(view);
    chrome::darken_menu(lighting);
    chrome::darken_menu(build);
    chrome::darken_menu(play);
    chrome::darken_menu(help);
    if recent_projects.is_null() || recent_levels.is_null() {
        return Err("recent menus were not created".into());
    }
    Ok((menu, view, lighting, recent_projects, recent_levels))
}

unsafe fn separator(menu: HMENU) {
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
}

unsafe fn append(menu: HMENU, id: usize, label: &str) {
    let text = wide(label);
    AppendMenuW(menu, MF_STRING, id, text.as_ptr());
    let count = GetMenuItemCount(menu);
    if count > 0 {
        chrome::own_menu_item(menu, (count - 1) as u32, label);
    }
}

unsafe fn popup(menu: HMENU, child: HMENU, label: &str) {
    let text = wide(label);
    AppendMenuW(menu, MF_POPUP, child as usize, text.as_ptr());
    let count = GetMenuItemCount(menu);
    if count > 0 {
        chrome::own_menu_item(menu, (count - 1) as u32, label);
    }
}

unsafe extern "system" fn status_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_ERASEBKGND {
        return 1;
    }
    if message != WM_PAINT {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let mut paint: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut paint);
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let width = (client.right - client.left).max(1);
    let height = (client.bottom - client.top).max(1);
    let memory = CreateCompatibleDC(hdc);
    let bitmap = CreateCompatibleBitmap(hdc, width, height);
    if !memory.is_null() && !bitmap.is_null() {
        let previous = SelectObject(memory, bitmap as _);
        let chrome = CreateSolidBrush(chrome::CHROME);
        FillRect(memory, &RECT { left: 0, top: 0, right: width, bottom: height }, chrome);
        DeleteObject(chrome);
        if let Some(editor) = editor_from(GetParent(hwnd)) {
            if !editor.ui_font.is_null() {
                SelectObject(memory, editor.ui_font as _);
            }
        }
        SetBkMode(memory, TRANSPARENT as i32);
        SetTextColor(memory, chrome::TEXT);
        let text = window_text(hwnd);
        let wide_text = wide(&text);
        let length = wide_text.len().saturating_sub(1) as i32;
        if length > 0 {
            let mut size = SIZE { cx: 0, cy: 0 };
            GetTextExtentPoint32W(memory, wide_text.as_ptr(), length, &mut size);
            let y = ((height - size.cy) / 2).max(0);
            TextOutW(memory, 8, y, wide_text.as_ptr(), length);
        }
        BitBlt(hdc, 0, 0, width, height, memory, 0, 0, SRCCOPY);
        SelectObject(memory, previous);
        DeleteObject(bitmap as _);
        DeleteDC(memory);
    }
    EndPaint(hwnd, &paint);
    0
}

unsafe extern "system" fn toolbar_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let editor = editor_from(GetParent(hwnd));
    match message {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            if let Some(editor) = editor {
                editor.paint_toolbar(hwnd);
            }
            0
        }
        WM_MOUSEMOVE => {
            if let Some(editor) = editor {
                let x = (lparam & 0xffff) as u16 as i16 as i32;
                let y = ((lparam >> 16) & 0xffff) as u16 as i16 as i32;
                let hot = editor.toolbar_icons.hit(editor.dpi, x, y);
                if editor.toolbar_hot != hot {
                    editor.toolbar_hot = hot;
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
                let mut track = MouseTrack { size: std::mem::size_of::<MouseTrack>() as u32, flags: 2, hwnd, time: 0 };
                let _ = TrackMouseEvent(&mut track);
            }
            0
        }
        WM_MOUSELEAVE => {
            if let Some(editor) = editor {
                editor.toolbar_hot = None;
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            0
        }
        WM_LBUTTONDOWN => {
            if let Some(editor) = editor {
                let x = (lparam & 0xffff) as u16 as i16 as i32;
                let y = ((lparam >> 16) & 0xffff) as u16 as i16 as i32;
                if let Some(index) = editor.toolbar_icons.hit(editor.dpi, x, y) {
                    editor.toolbar_click(index);
                }
            }
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe extern "system" fn viewport_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(editor) = editor_from(GetParent(hwnd)) {
        if editor.on_viewport_input(hwnd, message, wparam, lparam) {
            return if message == WM_SETCURSOR { 1 } else { 0 };
        }
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}

unsafe extern "system" fn frame_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        set_window_user(hwnd, create.lpCreateParams as isize);
    }
    let editor = editor_from(hwnd);
    match message {
        WM_CREATE => {
            if let Some(editor) = editor {
                if let Err(error) = create_children(editor, hwnd) {
                    editor.error = Some(error);
                    PostQuitMessage(1);
                }
            }
            0
        }
        WM_MEASUREITEM => {
            let measure = &mut *(lparam as *mut windows_sys::Win32::UI::Controls::MEASUREITEMSTRUCT);
            if measure.CtlType == windows_sys::Win32::UI::Controls::ODT_MENU {
                chrome::measure_menu(measure);
                return 1;
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_DRAWITEM => {
            let draw = &*(lparam as *mut windows_sys::Win32::UI::Controls::DRAWITEMSTRUCT);
            if draw.CtlType == windows_sys::Win32::UI::Controls::ODT_MENU {
                let font = editor.map(|editor| editor.ui_font).unwrap_or(std::ptr::null_mut());
                chrome::paint_menu(draw, font);
                return 1;
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_ERASEBKGND => {
            let hdc = wparam as HDC;
            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            if let Some(editor) = editor {
                if !editor.status.is_null() {
                    let mut window: RECT = std::mem::zeroed();
                    GetWindowRect(editor.status, &mut window);
                    let mut origin = POINT { x: window.left, y: window.top };
                    ScreenToClient(hwnd, &mut origin);
                    ExcludeClipRect(hdc, origin.x, origin.y, origin.x + (window.right - window.left), origin.y + (window.bottom - window.top));
                }
            }
            FillRect(hdc, &rect, chrome::chrome_brush());
            1
        }
        WM_CTLCOLORSTATIC => {
            SetTextColor(wparam as HDC, chrome::TEXT);
            SetBkColor(wparam as HDC, chrome::CHROME);
            chrome::chrome_brush() as LRESULT
        }
        WM_SIZE | WM_MOVE => {
            if let Some(editor) = editor {
                if message == WM_SIZE {
                    editor.layout_needed = true;
                }
                if editor.progress.blocking {
                    editor.place_load_overlay();
                }
            }
            0
        }
        WM_DPICHANGED => {
            if lparam != 0 {
                let suggested = &*(lparam as *const RECT);
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    suggested.left,
                    suggested.top,
                    (suggested.right - suggested.left).max(1),
                    (suggested.bottom - suggested.top).max(1),
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            if let Some(editor) = editor {
                editor.layout_needed = true;
            }
            0
        }
        WM_PARENTNOTIFY => {
            if (wparam & 0xffff) as u32 == WM_LBUTTONDOWN {
                if let Some(editor) = editor {
                    editor.note_child_focus((wparam >> 16) as u32);
                }
            }
            0
        }
        WM_TIMER => {
            if let Some(editor) = editor {
                if editor.load_depth > 0 {
                    return 0;
                }
                if editor.renderer.is_none() {
                    if editor.boot_failed {
                        return 0;
                    }
                    editor.realize();
                    if let Err(error) = editor.boot_viewport() {
                        editor.boot_failed = true;
                        if editor.self_test {
                            editor.error = Some(error);
                            PostQuitMessage(1);
                        } else {
                            editor.fail_progress("Starting editor", &error);
                        }
                        return 0;
                    }
                    return 0;
                }
                let redrawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| editor.redraw()));
                match redrawn {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        eprintln!("EDITOR_FRAME {error}");
                        editor.error = Some(error);
                        editor.release_renderer();
                        PostQuitMessage(1);
                    }
                    Err(payload) => {
                        let message = panic_payload(payload.as_ref());
                        eprintln!("EDITOR_FRAME panicked: {message}");
                        editor.error = Some(format!("panicked: {message}"));
                        editor.release_renderer();
                        PostQuitMessage(1);
                    }
                }
            }
            0
        }
        WM_DOCK_TEST => {
            if let Some(editor) = editor {
                if let Err(error) = editor.on_dock_test(wparam) {
                    eprintln!("EDITOR_FRAME {error}");
                    editor.error = Some(error);
                    editor.release_renderer();
                    PostQuitMessage(1);
                }
            }
            0
        }
        WM_ACTIVATE => {
            if (wparam & 0xffff) == 0 {
                if let Some(editor) = editor {
                    editor.end_capture(true, true);
                }
            }
            0
        }
        WM_KEYDOWN | WM_KEYUP => {
            if let Some(editor) = editor {
                if editor.on_nav_key(message, wparam, lparam) {
                    return 0;
                }
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_COMMAND => {
            if let Some(editor) = editor {
                editor.on_command((wparam & 0xffff) as usize, (wparam >> 16) as u32);
            }
            0
        }
        WM_CLOSE => {
            if let Some(editor) = editor {
                if editor.lod_capture {
                    println!("LOD_WM_CLOSE phase={}", editor.lod_phase);
                }
                KillTimer(hwnd, TIMER_FRAME);
                editor.release_renderer();
            }
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            if let Some(editor) = editor {
                KillTimer(hwnd, TIMER_FRAME);
                editor.release_renderer();
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe extern "system" fn dock_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        set_window_user(hwnd, create.lpCreateParams as isize);
    }
    let editor = editor_from(hwnd);
    match message {
        WM_ERASEBKGND => {
            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            FillRect(wparam as HDC, &rect, chrome::panel_brush());
            1
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
            chrome::control_color(message, wparam as HDC).map(|brush| brush as LRESULT).unwrap_or_else(|| DefWindowProcW(hwnd, message, wparam, lparam))
        }
        WM_PAINT => {
            if let Some(editor) = editor {
                editor.paint_dock(hwnd);
            }
            0
        }
        WM_LBUTTONDOWN | WM_MOUSEMOVE | WM_LBUTTONUP => {
            if let Some(editor) = editor {
                editor.dock_mouse(message, lparam);
            }
            0
        }
        WM_SETCURSOR => {
            if let Some(editor) = editor {
                if editor.cursor_for_dock() {
                    return 1;
                }
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_PARENTNOTIFY => {
            if (wparam & 0xffff) as u32 == WM_LBUTTONDOWN {
                if let Some(editor) = editor {
                    editor.note_child_focus((wparam >> 16) as u32);
                }
            }
            0
        }
        WM_NOTIFY => {
            if let Some(editor) = editor {
                editor.on_outliner_notify(lparam)
            } else {
                0
            }
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

static mut INSPECTOR_EDIT_PROC: isize = 0;

unsafe fn subclass_inspector_edit(hwnd: HWND) {
    let current = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
    if INSPECTOR_EDIT_PROC == 0 {
        INSPECTOR_EDIT_PROC = current;
    }
    windows_sys::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(hwnd, GWLP_WNDPROC, inspector_edit_proc as *const () as isize);
}

unsafe extern "system" fn inspector_edit_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_KEYDOWN && (wparam == VK_RETURN as usize || wparam == VK_ESCAPE as usize || wparam == 0x26 || wparam == 0x28) {
        if let Some(editor) = editor_from(GetParent(hwnd)) {
            if !editor.inspector_applying {
                if wparam == VK_ESCAPE as usize {
                    editor.revert_inspector_control(hwnd);
                } else if wparam == 0x26 || wparam == 0x28 {
                    editor.step_inspector_edit(hwnd, if wparam == 0x26 { 1.0 } else { -1.0 });
                } else {
                    editor.commit_inspector_control(hwnd);
                }
            }
        }
        return 0;
    }
    if message == WM_LBUTTONDOWN {
        if let Some(editor) = editor_from(GetParent(hwnd)) {
            editor.scrub_inspector_edit(hwnd, (lparam & 0xffff) as i16 as i32, true);
        }
    } else if message == WM_MOUSEMOVE {
        if GetKeyState(0x01) < 0 {
            if let Some(editor) = editor_from(GetParent(hwnd)) {
                if editor.scrub_hwnd == hwnd {
                    editor.scrub_inspector_edit(hwnd, (lparam & 0xffff) as i16 as i32, false);
                    if editor.scrubbing {
                        return 0;
                    }
                }
            }
        }
    } else if message == WM_LBUTTONUP {
        if let Some(editor) = editor_from(GetParent(hwnd)) {
            if editor.scrubbing && editor.scrub_hwnd == hwnd {
                editor.scrubbing = false;
                editor.scrub_hwnd = std::ptr::null_mut();
                ReleaseCapture();
                editor.commit_inspector_control(hwnd);
                return 0;
            }
        }
    }
    let previous = std::mem::transmute::<isize, unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>(INSPECTOR_EDIT_PROC);
    CallWindowProcW(Some(previous), hwnd, message, wparam, lparam)
}

unsafe extern "system" fn inspector_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        set_window_user(hwnd, create.lpCreateParams as isize);
    }
    let editor = editor_from(hwnd);
    match message {
        WM_ERASEBKGND => {
            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            FillRect(wparam as HDC, &rect, chrome::panel_brush());
            1
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLORLISTBOX => {
            chrome::control_color(message, wparam as HDC).map(|brush| brush as LRESULT).unwrap_or_else(|| DefWindowProcW(hwnd, message, wparam, lparam))
        }
        WM_SIZE => {
            if let Some(editor) = editor {
                editor.layout_inspector_controls();
            }
            0
        }
        WM_INSPECTOR_REFRESH => {
            if let Some(editor) = editor {
                editor.flush_inspector_refresh();
            }
            0
        }
        WM_VSCROLL => {
            if let Some(editor) = editor {
                editor.on_inspector_scroll(wparam);
            }
            0
        }
        WM_MOUSEWHEEL => {
            if let Some(editor) = editor {
                editor.on_inspector_wheel(wparam);
            }
            0
        }
        WM_COMMAND => {
            let notify = (wparam >> 16) as u32;
            if let Some(editor) = editor {
                if notify == EN_SETFOCUS || notify == CBN_SETFOCUS || notify == 0 {
                    editor.note_child_focus(INSPECTOR.raw());
                }
                if notify == EN_KILLFOCUS && !editor.inspector_applying {
                    editor.commit_inspector_control(lparam as HWND);
                } else if (notify == 0 || notify == CBN_SELCHANGE) && !editor.inspector_applying {
                    editor.on_inspector_command(lparam as HWND);
                }
            }
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn inspector_host(parent: HWND, instance: HINSTANCE, param: *const core::ffi::c_void) -> Result<HWND, String> {
    let class_name = wide(CLASS_INSPECTOR);
    let title = wide("Inspector");
    let hwnd = CreateWindowExW(
        0,
        class_name.as_ptr(),
        title.as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS | WS_VSCROLL,
        0,
        0,
        10,
        10,
        parent,
        INSPECTOR.raw() as usize as HMENU,
        instance,
        param,
    );
    if hwnd.is_null() {
        Err("inspector host was not created".into())
    } else {
        Ok(hwnd)
    }
}

unsafe fn create_inspector_child(parent: HWND, class_name: &str, text: &str, extra: u32) -> HWND {
    let instance = GetModuleHandleW(std::ptr::null());
    child(parent, instance, class_name, text, extra, 0).unwrap_or(std::ptr::null_mut())
}

/// Interior of the empty bar drawn in `assets/jarvigSplash.png` (1672×940).
const SPLASH_BAR: (f32, f32, f32, f32) = (476.0, 771.0, 1198.0, 793.0);

fn splash_file() -> Option<std::path::PathBuf> {
    let bundled = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/jarvigSplash.png");
    if bundled.is_file() {
        return Some(bundled);
    }
    let mut dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    for _ in 0..8 {
        let candidate = dir.join("assets/jarvigSplash.png");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

fn ensure_splash(editor: &mut Editor) {
    if editor.splash_loaded {
        return;
    }
    editor.splash_loaded = true;
    let Some(path) = splash_file() else {
        editor.append("Splash art was not found at assets/jarvigSplash.png. The load panel stays plain.");
        return;
    };
    let Ok(bytes) = std::fs::read(&path) else {
        editor.append("Splash art could not be read. The load panel stays plain.");
        return;
    };
    let Ok((width, height, rgba)) = crate::png_decode::decode_png_rgba8(&bytes) else {
        editor.append("Splash art is not a readable PNG. The load panel stays plain.");
        return;
    };
    let mut bgra = Vec::with_capacity(rgba.len());
    for pixel in rgba.chunks_exact(4) {
        bgra.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
    }
    editor.splash_pixels = bgra;
    editor.splash_size = (width as i32, height as i32);
    cache_splash_bitmap(editor);
    editor.append(&format!("Splash art {width}x{height} from {}.", path.display()));
}

fn cache_splash_bitmap(editor: &mut Editor) {
    if !editor.splash_dc.is_null() {
        return;
    }
    let (width, height) = editor.splash_size;
    let pixels = (width as usize).saturating_mul(height as usize).saturating_mul(4);
    if width <= 0 || height <= 0 || editor.splash_pixels.len() != pixels {
        return;
    }
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let dc = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        if dc.is_null() || bitmap.is_null() {
            if !bitmap.is_null() {
                DeleteObject(bitmap as _);
            }
            if !dc.is_null() {
                DeleteDC(dc);
            }
            if !screen.is_null() {
                ReleaseDC(std::ptr::null_mut(), screen);
            }
            return;
        }
        SelectObject(dc, bitmap as _);
        let info = splash_bitmap_info(width, height);
        SetDIBitsToDevice(
            dc,
            0,
            0,
            width as u32,
            height as u32,
            0,
            0,
            0,
            height as u32,
            editor.splash_pixels.as_ptr().cast(),
            &info,
            DIB_RGB_COLORS,
        );
        if !screen.is_null() {
            ReleaseDC(std::ptr::null_mut(), screen);
        }
        editor.splash_dc = dc;
        editor.splash_bitmap = bitmap;
    }
}

fn splash_bitmap_info(width: i32, height: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [windows_sys::Win32::Graphics::Gdi::RGBQUAD { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbReserved: 0 }],
    }
}

fn splash_bar_rect(source: (i32, i32), client_w: i32, client_h: i32) -> RECT {
    let (source_w, source_h) = source;
    if source_w <= 0 || source_h <= 0 {
        return RECT { left: 48, top: client_h * 3 / 4, right: (client_w - 48).max(49), bottom: client_h * 3 / 4 + 14 };
    }
    RECT {
        left: SPLASH_BAR.0 as i32,
        top: SPLASH_BAR.1 as i32,
        right: SPLASH_BAR.2 as i32,
        bottom: SPLASH_BAR.3 as i32,
    }
}

fn paint_centered(hdc: HDC, text: &str, center_x: i32, y: i32) {
    if text.is_empty() {
        return;
    }
    let wide_text = wide(text);
    let length = wide_text.len().saturating_sub(1) as i32;
    let mut size = SIZE { cx: 0, cy: 0 };
    unsafe {
        GetTextExtentPoint32W(hdc, wide_text.as_ptr(), length, &mut size);
        TextOutW(hdc, center_x - size.cx / 2, y, wide_text.as_ptr(), length);
    }
}

unsafe fn paint_load_into(hdc: HDC, editor: &Editor, client_w: i32, client_h: i32) {
    let client = RECT { left: 0, top: 0, right: client_w, bottom: client_h };
    let backdrop = CreateSolidBrush(chrome::rgb(6, 10, 16));
    FillRect(hdc, &client, backdrop);
    DeleteObject(backdrop);
    let (source_w, source_h) = editor.splash_size;
    if source_w > 0 && source_h > 0 && !editor.splash_dc.is_null() && !editor.splash_bitmap.is_null() {
        BitBlt(hdc, 0, 0, source_w.min(client_w), source_h.min(client_h), editor.splash_dc, 0, 0, SRCCOPY);
    } else if source_w > 0 && source_h > 0 && editor.splash_pixels.len() == (source_w as usize) * (source_h as usize) * 4 {
        let info = splash_bitmap_info(source_w, source_h);
        SetDIBitsToDevice(
            hdc,
            0,
            0,
            source_w as u32,
            source_h as u32,
            0,
            0,
            0,
            source_h as u32,
            editor.splash_pixels.as_ptr().cast(),
            &info,
            DIB_RGB_COLORS,
        );
    }
    let bar = splash_bar_rect(editor.splash_size, client_w, client_h);
    let fraction = editor.progress.fraction;
    let spinner = editor.progress.spinner;
    let span = (bar.right - bar.left).max(1);
    let (fill_left, fill_right) = match fraction {
        Some(value) => (bar.left, bar.left + ((span as f32) * value.clamp(0.0, 1.0)) as i32),
        None => {
            let chunk = (span / 5).clamp(24, 160);
            let travel = (span - chunk).max(1);
            let left = bar.left + ((spinner as i32).wrapping_mul(8)).rem_euclid(travel);
            (left, left + chunk)
        }
    };
    let fill_rect = RECT {
        left: fill_left.max(bar.left),
        top: bar.top,
        right: fill_right.min(bar.right).max(bar.left + 2),
        bottom: bar.bottom,
    };
    let accent = CreateSolidBrush(chrome::ACCENT);
    FillRect(hdc, &fill_rect, accent);
    DeleteObject(accent);
    if !editor.ui_font.is_null() {
        SelectObject(hdc, editor.ui_font as _);
    }
    SetBkMode(hdc, TRANSPARENT as i32);
    let failed = editor.progress.failed.is_some();
    let phase = if failed { format!("Failed while {}", editor.progress.phase) } else { editor.progress.phase.clone() };
    let detail = editor.progress.detail.clone();
    SetTextColor(hdc, if failed { chrome::rgb(255, 176, 160) } else { chrome::TEXT });
    let center = (bar.left + bar.right) / 2;
    let text_y = bar.bottom + 14;
    paint_centered(hdc, &phase, center, text_y);
    SetTextColor(hdc, chrome::TEXT_DIM);
    paint_centered(hdc, &detail, center, text_y + 22);
}

unsafe extern "system" fn load_overlay_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_MOUSEACTIVATE {
        return 3;
    }
    if message == WM_ERASEBKGND {
        return 1;
    }
    if message != WM_PAINT {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let Some(editor) = editor_from(GetParent(hwnd)) else {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    };
    ensure_splash(editor);
    let mut paint: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut paint);
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let client_w = (client.right - client.left).max(1);
    let client_h = (client.bottom - client.top).max(1);
    let memory = CreateCompatibleDC(hdc);
    let bitmap = CreateCompatibleBitmap(hdc, client_w, client_h);
    if !memory.is_null() && !bitmap.is_null() {
        let previous = SelectObject(memory, bitmap as _);
        paint_load_into(memory, editor, client_w, client_h);
        BitBlt(hdc, 0, 0, client_w, client_h, memory, 0, 0, SRCCOPY);
        SelectObject(memory, previous);
        DeleteObject(bitmap as _);
        DeleteDC(memory);
    } else {
        if !bitmap.is_null() {
            DeleteObject(bitmap as _);
        }
        if !memory.is_null() {
            DeleteDC(memory);
        }
        paint_load_into(hdc, editor, client_w, client_h);
    }
    EndPaint(hwnd, &paint);
    0
}

unsafe extern "system" fn drop_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_ERASEBKGND {
        let mut rect: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut rect);
        let brush = CreateSolidBrush(chrome::ACCENT);
        FillRect(wparam as HDC, &rect, brush);
        DeleteObject(brush);
        return 1;
    }
    DefWindowProcW(hwnd, message, wparam, lparam)
}

fn content_ground_hit(ray: &jarvig_core::PickRay) -> jarvig_core::Vec3 {
    let direction_y = ray.direction.y;
    if direction_y.abs() < 1.0e-4 {
        return jarvig_core::Vec3::new(ray.origin.x, 0.4, ray.origin.z - 2.0);
    }
    let t = (0.0 - ray.origin.y) / direction_y;
    if t < 0.05 {
        return jarvig_core::Vec3::new(ray.origin.x, 0.4, ray.origin.z - 2.0);
    }
    jarvig_core::Vec3::new(ray.origin.x + ray.direction.x * t, 0.0, ray.origin.z + ray.direction.z * t)
}

unsafe extern "system" fn content_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_CREATE {
        if let Some(editor) = editor_from(GetParent(hwnd)) {
            let instance = GetModuleHandleW(std::ptr::null());
            let class_name = wide("EDIT");
            let search = CreateWindowExW(
                0,
                class_name.as_ptr(),
                std::ptr::null(),
                WS_CHILD | WS_VISIBLE | 0x00800000 | 0x0080,
                8,
                28,
                200,
                22,
                hwnd,
                4401 as HMENU,
                instance,
                std::ptr::null(),
            );
            editor.content_search = search;
            chrome::use_dark_control(search);
        }
        return 0;
    }
    let Some(editor) = editor_from(GetParent(hwnd)) else {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    };
    match message {
        WM_SIZE => {
            if !editor.content_search.is_null() {
                let width = (lparam & 0xffff) as i32;
                SetWindowPos(editor.content_search, std::ptr::null_mut(), 8, 28, (width - 16).max(40), 22, SWP_NOZORDER);
            }
            0
        }
        WM_COMMAND => {
            if (wparam & 0xffff) == 4401 && (wparam >> 16) == 0x0300 {
                let mut buffer = [0u16; 160];
                let count = GetWindowTextW(editor.content_search, buffer.as_mut_ptr(), buffer.len() as i32).max(0) as usize;
                editor.browser.query = String::from_utf16_lossy(&buffer[..count.min(buffer.len())]);
                editor.browser.scroll = 0;
                InvalidateRect(hwnd, std::ptr::null(), 1);
            }
            0
        }
        WM_LBUTTONDOWN => {
            SetFocus(hwnd);
            let x = (lparam & 0xffff) as u16 as i16 as i32;
            let y = ((lparam >> 16) & 0xffff) as u16 as i16 as i32;
            if y < 26 {
                let chip = (x / 78).max(0) as usize;
                if chip < content_browser::FILTERS.len() {
                    editor.browser.filter = chip;
                    editor.browser.scroll = 0;
                    InvalidateRect(hwnd, std::ptr::null(), 1);
                }
                return 0;
            }
            if let Some(row) = editor.browser.rows(56, 34).into_iter().find(|row| y >= row.y && y < row.y + 34) {
                let id = editor.browser.assets[row.index].id;
                editor.browser.selected = Some(id);
                editor.content_press = Some((x, y, id));
                SetCapture(hwnd);
                InvalidateRect(hwnd, std::ptr::null(), 1);
            }
            0
        }
        WM_MOUSEMOVE => {
            if let Some((origin_x, origin_y, id)) = editor.content_press {
                let x = (lparam & 0xffff) as u16 as i16 as i32;
                let y = ((lparam >> 16) & 0xffff) as u16 as i16 as i32;
                if (x - origin_x).abs() + (y - origin_y).abs() > 4 {
                    editor.content_drag = Some(id);
                    editor.update_content_drag();
                }
            }
            0
        }
        WM_LBUTTONDBLCLK => {
            let y = ((lparam >> 16) & 0xffff) as u16 as i16 as i32;
            editor.content_press = None;
            editor.content_drag = None;
            if let Some(row) = editor.browser.rows(56, 34).into_iter().find(|row| y >= row.y && y < row.y + 34) {
                let asset = editor.browser.assets[row.index].clone();
                if asset.kind == "Character" {
                    if let Err(error) = editor.open_character_document(&asset) {
                        editor.append(&error);
                    }
                } else if asset.kind == "Prefab" {
                    editor.open_character_asset(&asset);
                }
            }
            0
        }
        WM_LBUTTONUP => {
            ReleaseCapture();
            if editor.content_drag.is_some() {
                editor.drop_content_asset();
            }
            editor.content_press = None;
            editor.content_drag = None;
            InvalidateRect(hwnd, std::ptr::null(), 1);
            0
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) as i16) as i32;
            editor.browser.scroll = (editor.browser.scroll - delta / 2).max(0);
            InvalidateRect(hwnd, std::ptr::null(), 1);
            0
        }
        WM_PAINT => {
            let mut paint: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut paint);
            let mut client: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut client);
            let background = CreateSolidBrush(chrome::PANEL);
            FillRect(hdc, &client, background);
            DeleteObject(background);
            SetBkMode(hdc, TRANSPARENT as i32);
            let font = GetStockObject(DEFAULT_GUI_FONT as i32);
            SelectObject(hdc, font);
            SetTextColor(hdc, chrome::TEXT);
            for (index, label) in content_browser::FILTERS.iter().enumerate() {
                let selected = index == editor.browser.filter;
                let mut chip = RECT { left: 4 + index as i32 * 78, top: 4, right: 4 + index as i32 * 78 + 74, bottom: 24 };
                let brush = CreateSolidBrush(if selected { chrome::SELECT } else { chrome::BUTTON });
                FillRect(hdc, &chip, brush);
                DeleteObject(brush);
                SetTextColor(hdc, if selected { chrome::TEXT } else { chrome::TEXT_DIM });
                chip.left += 6;
                chip.top += 4;
                let text = wide(label);
                TextOutW(hdc, chip.left, chip.top, text.as_ptr(), (text.len() - 1) as i32);
            }
            SetTextColor(hdc, chrome::TEXT);
            if editor.browser.assets.is_empty() {
                let text = wide("Scanning project content, or no project is open.");
                TextOutW(hdc, 8, 64, text.as_ptr(), (text.len() - 1) as i32);
            }
            let rows = editor.browser.rows(56, 34);
            for row in rows {
                if row.y + 34 < 56 || row.y > client.bottom - 24 {
                    continue;
                }
                let asset = &editor.browser.assets[row.index];
                let selected = editor.browser.selected == Some(asset.id);
                if selected {
                    let bar = RECT { left: 4, top: row.y, right: client.right - 4, bottom: row.y + 32 };
                    let brush = CreateSolidBrush(chrome::SELECT_SOFT);
                    FillRect(hdc, &bar, brush);
                    DeleteObject(brush);
                }
                if let Some(thumb) = editor.browser.thumb(asset.id) {
                    paint_thumb(hdc, 8, row.y + 2, thumb);
                }
                let label = wide(&format!("{}   {} / {}", asset.name, asset.kind, asset.subtype));
                TextOutW(hdc, 40, row.y + 8, label.as_ptr(), (label.len() - 1) as i32);
            }
            let footer = editor.browser.footer();
            let footer_text = wide(&footer);
            SetTextColor(hdc, chrome::TEXT_DIM);
            TextOutW(hdc, 8, client.bottom - 18, footer_text.as_ptr(), (footer_text.len() - 1) as i32);
            EndPaint(hwnd, &paint);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn paint_thumb(hdc: HDC, x: i32, y: i32, thumb: &content_browser::Thumb) {
    if thumb.width == 0 || thumb.height == 0 || thumb.rgba.len() < thumb.width as usize * thumb.height as usize * 4 {
        return;
    }
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: thumb.width as i32,
            biHeight: -(thumb.height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [windows_sys::Win32::Graphics::Gdi::RGBQUAD { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbReserved: 0 }],
    };
    let mut bgra = Vec::with_capacity(thumb.rgba.len());
    for pixel in thumb.rgba.chunks_exact(4) {
        bgra.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
    }
    SetDIBitsToDevice(hdc, x, y, 28, 28, 0, 0, 0, thumb.height, bgra.as_ptr().cast(), &mut info, DIB_RGB_COLORS);
}

unsafe fn create_children(editor: &mut Editor, frame: HWND) -> Result<(), String> {
    editor.frame = frame;
    let instance = GetModuleHandleW(std::ptr::null());
    chrome::enable_dark_caption(frame);
    editor.toolbar = child_class(frame, instance, CLASS_TOOLBAR, std::ptr::null())?;
    editor.dock_host = child_class(frame, instance, CLASS_DOCK, editor as *mut Editor as *const _)?;
    editor.highlight = child_class(editor.dock_host, instance, CLASS_DROP, std::ptr::null())?;
    unsafe { ShowWindow(editor.highlight, SW_HIDE); }
    editor.panels = [
        (OUTLINER, outliner_tree(editor.dock_host, instance)?),
        (INSPECTOR, inspector_host(editor.dock_host, instance, editor as *mut Editor as *const _)?),
        (PERSPECTIVE, panel_class(frame, instance, PERSPECTIVE, CLASS_VIEWPORT)?),
        (CONTENT, panel_class(editor.dock_host, instance, CONTENT, CLASS_CONTENT)?),
        (
            OUTPUT,
            panel_control(editor.dock_host, instance, OUTPUT, "EDIT", "", WS_VSCROLL | ES_MULTILINE | ES_READONLY)?,
        ),
    ];
    editor.viewport_born = editor.panel_hwnd(PERSPECTIVE);
    editor.status = child_class(frame, instance, CLASS_STATUS, std::ptr::null())?;
    editor.set_status("Starting editor");
    editor.dpi = GetDpiForWindow(frame).max(96);
    if !editor.self_test {
        editor.load_overlay = load_popup(frame, instance)?;
    }
    editor.progress.begin("Starting editor", "The window is up.", None);
    editor.place_load_overlay();
    editor.ensure_font();
    chrome::use_dark_control(editor.panel_hwnd(CONTENT));
    chrome::use_dark_control(editor.panel_hwnd(OUTPUT));
    chrome::use_dark_control(editor.panel_hwnd(INSPECTOR));
    editor.sync_view_menu();
    editor.append("JARVIGEditor");
    editor.append("The engine is in this process. The dock tree is not the window tree.");
    ensure_splash(editor);
    editor.sync_outliner();
    SetTimer(frame, TIMER_FRAME, 16, None);
    Ok(())
}

unsafe fn child(parent: HWND, instance: HINSTANCE, class_name: &str, text: &str, extra: u32, id: usize) -> Result<HWND, String> {
    let class = wide(class_name);
    let text = wide(text);
    let hwnd = CreateWindowExW(
        0,
        class.as_ptr(),
        text.as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | extra,
        0,
        0,
        10,
        10,
        parent,
        id as HMENU,
        instance,
        std::ptr::null(),
    );
    if hwnd.is_null() {
        Err(format!("panel {class_name} was not created"))
    } else {
        Ok(hwnd)
    }
}

unsafe fn load_popup(owner: HWND, instance: HINSTANCE) -> Result<HWND, String> {
    let class_name = wide(CLASS_LOAD);
    let hwnd = CreateWindowExW(
        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
        class_name.as_ptr(),
        std::ptr::null(),
        WS_POPUP | WS_CLIPSIBLINGS,
        0,
        0,
        10,
        10,
        owner,
        std::ptr::null_mut(),
        instance,
        std::ptr::null(),
    );
    if hwnd.is_null() {
        Err("load overlay was not created".into())
    } else {
        Ok(hwnd)
    }
}

unsafe fn child_class(parent: HWND, instance: HINSTANCE, class: &str, param: *const core::ffi::c_void) -> Result<HWND, String> {
    child_class_ex(parent, instance, class, param, 0)
}

unsafe fn child_class_ex(parent: HWND, instance: HINSTANCE, class: &str, param: *const core::ffi::c_void, ex_style: u32) -> Result<HWND, String> {
    let class_name = wide(class);
    let hwnd = CreateWindowExW(
        ex_style,
        class_name.as_ptr(),
        std::ptr::null(),
        WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
        0,
        0,
        10,
        10,
        parent,
        std::ptr::null_mut(),
        instance,
        param,
    );
    if hwnd.is_null() {
        Err(format!("{class} was not created"))
    } else {
        Ok(hwnd)
    }
}

unsafe fn panel_control(
    parent: HWND,
    instance: HINSTANCE,
    id: PanelId,
    class_name: &str,
    text: &str,
    extra: u32,
) -> Result<HWND, String> {
    child(parent, instance, class_name, text, extra, id.raw() as usize)
}

unsafe fn panel_class(parent: HWND, instance: HINSTANCE, id: PanelId, class: &str) -> Result<HWND, String> {
    let class_name = wide(class);
    let hwnd = CreateWindowExW(
        0,
        class_name.as_ptr(),
        std::ptr::null(),
        WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
        0,
        0,
        10,
        10,
        parent,
        id.raw() as HMENU,
        instance,
        std::ptr::null(),
    );
    if hwnd.is_null() {
        Err("viewport panel was not created".into())
    } else {
        Ok(hwnd)
    }
}

unsafe fn editor_from<'a>(hwnd: HWND) -> Option<&'a mut Editor> {
    let pointer = get_window_user(hwnd);
    if pointer == 0 {
        None
    } else {
        Some(&mut *(pointer as *mut Editor))
    }
}

unsafe fn set_window_user(hwnd: HWND, value: isize) {
    windows_sys::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(hwnd, GWLP_USERDATA, value);
}

unsafe fn get_window_user(hwnd: HWND) -> isize {
    windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA)
}
