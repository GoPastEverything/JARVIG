//! Editor chrome. Dark shell colors and the navigation toolbar.
//! Each button is an icon with a short caption under it.
//!
//! Icons are source PNGs under `assets/editor/ui/navigation`. Windows Imaging
//! decodes them. They are not scene assets and not an entity.

use std::path::PathBuf;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{HWND, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::GetTextExtentPoint32W;
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_CHECKED, ODS_SELECTED, ODT_MENU};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SetMenuInfo, SetMenuItemInfoW, MENUINFO, MENUITEMINFOW, MFT_OWNERDRAW, MIIM_DATA, MIIM_FTYPE, MIM_APPLYTOSUBMENUS, MIM_BACKGROUND,
};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, FillRect, SelectObject, SetBkMode, SetDIBitsToDevice, SetTextColor, TextOutW, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, CLEARTYPE_QUALITY, DEFAULT_CHARSET, DIB_RGB_COLORS, FW_NORMAL, HBRUSH, HDC, HFONT,
};
use windows_sys::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::Controls::SetWindowTheme;

pub const CHROME: u32 = rgb(24, 24, 24);
pub const PANEL: u32 = rgb(36, 36, 36);
pub const FIELD: u32 = rgb(22, 22, 22);
pub const TAB_IDLE: u32 = rgb(28, 28, 28);
pub const BUTTON: u32 = rgb(42, 42, 42);
pub const BUTTON_HOT: u32 = rgb(58, 58, 58);
pub const BUTTON_ON: u32 = rgb(18, 18, 18);
pub const TEXT: u32 = rgb(220, 220, 220);
pub const TEXT_DIM: u32 = rgb(148, 148, 148);
pub const ACCENT: u32 = rgb(74, 180, 255);
pub const SELECT: u32 = rgb(0, 102, 176);
pub const SELECT_SOFT: u32 = rgb(0, 74, 130);
pub const SPLITTER: u32 = rgb(14, 14, 14);
pub const LINE: u32 = rgb(64, 64, 64);

const WM_CTLCOLOREDIT: u32 = 0x0133;
const GENERIC_READ: u32 = 0x8000_0000;

pub const fn rgb(red: u32, green: u32, blue: u32) -> u32 {
    red | (green << 8) | (blue << 16)
}

struct Brushes {
    chrome: isize,
    panel: isize,
    field: isize,
    button: isize,
    button_hot: isize,
    button_on: isize,
    accent: isize,
    line: isize,
}

fn brushes() -> &'static Brushes {
    use std::sync::OnceLock;
    static BRUSHES: OnceLock<Brushes> = OnceLock::new();
    BRUSHES.get_or_init(|| unsafe {
        Brushes {
            chrome: CreateSolidBrush(CHROME) as isize,
            panel: CreateSolidBrush(PANEL) as isize,
            field: CreateSolidBrush(FIELD) as isize,
            button: CreateSolidBrush(BUTTON) as isize,
            button_hot: CreateSolidBrush(BUTTON_HOT) as isize,
            button_on: CreateSolidBrush(BUTTON_ON) as isize,
            accent: CreateSolidBrush(ACCENT) as isize,
            line: CreateSolidBrush(LINE) as isize,
        }
    })
}

pub fn chrome_brush() -> HBRUSH {
    brushes().chrome as HBRUSH
}

pub fn panel_brush() -> HBRUSH {
    brushes().panel as HBRUSH
}

pub fn field_brush() -> HBRUSH {
    brushes().field as HBRUSH
}

fn brush(slot: isize) -> HBRUSH {
    slot as HBRUSH
}

/// Ask Windows to draw menus and captions with the dark app theme. Missing exports are ignored.
pub fn enable_dark_app() -> bool {
    unsafe {
        let name = wide("uxtheme.dll");
        let theme = LoadLibraryW(name.as_ptr());
        if theme.is_null() {
            return false;
        }
        let mut armed = false;
        if let Some(set_mode) = proc_ordinal(theme, 135) {
            let set_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(set_mode);
            set_mode(2);
            armed = true;
        }
        if let Some(refresh) = proc_ordinal(theme, 104) {
            let refresh: unsafe extern "system" fn() = std::mem::transmute(refresh);
            refresh();
        }
        if let Some(flush) = proc_ordinal(theme, 136) {
            let flush: unsafe extern "system" fn() = std::mem::transmute(flush);
            flush();
        }
        armed
    }
}

pub fn enable_dark_caption(hwnd: HWND) {
    let dark = 1i32;
    let caption = CHROME;
    let text = TEXT;
    unsafe {
        let _ = DwmSetWindowAttribute(hwnd, 20, &dark as *const i32 as *const _, 4);
        let _ = DwmSetWindowAttribute(hwnd, 19, &dark as *const i32 as *const _, 4);
        let _ = DwmSetWindowAttribute(hwnd, 35, &caption as *const u32 as *const _, 4);
        let _ = DwmSetWindowAttribute(hwnd, 36, &text as *const u32 as *const _, 4);
        let _ = DwmSetWindowAttribute(hwnd, 34, &caption as *const u32 as *const _, 4);
        use_dark_control(hwnd);
        let name = wide("uxtheme.dll");
        let theme = LoadLibraryW(name.as_ptr());
        if !theme.is_null() {
            if let Some(allow) = proc_ordinal(theme, 133) {
                let allow: unsafe extern "system" fn(HWND, i32) -> i32 = std::mem::transmute(allow);
                allow(hwnd, 1);
            }
            if let Some(flush) = proc_ordinal(theme, 136) {
                let flush: unsafe extern "system" fn() = std::mem::transmute(flush);
                flush();
            }
        }
        let _ = windows_sys::Win32::UI::WindowsAndMessaging::DrawMenuBar(hwnd);
    }
}

pub fn use_dark_control(hwnd: HWND) {
    if hwnd.is_null() {
        return;
    }
    let theme = wide("DarkMode_Explorer");
    unsafe {
        let _ = SetWindowTheme(hwnd, theme.as_ptr(), std::ptr::null());
    }
}

pub fn ui_font(dpi: u32) -> HFONT {
    let height = -((((9 * dpi.max(96)) + 36) / 72).max(1) as i32);
    let face = wide("Segoe UI");
    let font = unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            face.as_ptr(),
        )
    };
    if font.is_null() {
        unsafe { windows_sys::Win32::Graphics::Gdi::GetStockObject(windows_sys::Win32::Graphics::Gdi::DEFAULT_GUI_FONT) as HFONT }
    } else {
        font
    }
}

/// Edit and label colors for `WM_CTLCOLOR*`. Returns the background brush.
pub fn control_color(message: u32, hdc: HDC) -> Option<HBRUSH> {
    let (text, background, brush) = if message == WM_CTLCOLOREDIT || message == 0x0134 {
        (TEXT, FIELD, field_brush())
    } else if message == 0x0138 || message == 0x0135 {
        (TEXT, PANEL, panel_brush())
    } else {
        return None;
    };
    unsafe {
        SetTextColor(hdc, text);
        windows_sys::Win32::Graphics::Gdi::SetBkColor(hdc, background);
    }
    Some(brush)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToolbarCommand {
    Select,
    Translate,
    Rotate,
    Scale,
    Block,
    Extrude,
    Inset,
    Bevel,
    Subdivide,
    MoveEdge,
    ExtrudeEdge,
    SplitEdge,
    MoveVertex,
    Level,
    Land,
    Character,
    Play,
    Pause,
    Stop,
    Fly,
    Pan,
    Orbit,
    Focus,
    Snap,
    Local,
    Speed,
    Maximize,
}

/// Which editor workspace is showing. Not a second world.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WorkspaceMode {
    Level,
    Land,
    Character,
}

impl ToolbarCommand {
    pub fn is_object_tool(self) -> bool {
        matches!(self, Self::Select | Self::Translate | Self::Rotate | Self::Scale)
    }

    pub fn is_workspace(self) -> bool {
        matches!(self, Self::Level | Self::Land | Self::Character)
    }

    /// Shown only while a parametric solid is selected.
    pub fn is_modeling(self) -> bool {
        matches!(
            self,
            Self::Extrude | Self::Inset | Self::Bevel | Self::Subdivide | Self::MoveEdge | Self::ExtrudeEdge | Self::SplitEdge | Self::MoveVertex
        )
    }

    /// Log and menu name. Longer than the toolbar caption.
    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Translate => "Translate",
            Self::Rotate => "Rotate",
            Self::Scale => "Scale",
            Self::Block => "Block",
            Self::Extrude => "Extrude",
            Self::Inset => "Inset",
            Self::Bevel => "Bevel",
            Self::Subdivide => "Subdivide",
            Self::MoveEdge => "Move Edge",
            Self::ExtrudeEdge => "Extrude Edge",
            Self::SplitEdge => "Split Edge",
            Self::MoveVertex => "Move Vertex",
            Self::Level => "Level",
            Self::Land => "Land",
            Self::Character => "Character",
            Self::Play => "Play",
            Self::Pause => "Pause",
            Self::Stop => "Stop",
            Self::Fly => "Fly",
            Self::Pan => "Pan",
            Self::Orbit => "Orbit",
            Self::Focus => "Focus selected",
            Self::Snap => "Grid snap",
            Self::Local => "Local / world",
            Self::Speed => "Camera speed",
            Self::Maximize => "Maximize viewport",
        }
    }

    /// Short word drawn under the icon. Translate says Move. Local says World until toggled.
    pub fn caption(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Translate => "Move",
            Self::Rotate => "Rotate",
            Self::Scale => "Scale",
            Self::Block => "Block",
            Self::Extrude => "Extrude",
            Self::Inset => "Inset",
            Self::Bevel => "Bevel",
            Self::Subdivide => "Subdiv",
            Self::MoveEdge => "Edge",
            Self::ExtrudeEdge => "Extend",
            Self::SplitEdge => "Split",
            Self::MoveVertex => "Vertex",
            Self::Level => "Level",
            Self::Land => "Land",
            Self::Character => "Character",
            Self::Play => "Play",
            Self::Pause => "Pause",
            Self::Stop => "Stop",
            Self::Fly => "Fly",
            Self::Pan => "Pan",
            Self::Orbit => "Orbit",
            Self::Focus => "Focus",
            Self::Snap => "Snap",
            Self::Local => "World",
            Self::Speed => "Speed",
            Self::Maximize => "Max",
        }
    }

    pub fn caption_for(self, local_space: bool) -> &'static str {
        if self == Self::Local && local_space { "Local" } else { self.caption() }
    }
}

struct Icon {
    width: u32,
    height: u32,
    /// Premultiplied BGRA.
    pixels: Vec<u8>,
}

struct Button {
    command: ToolbarCommand,
    icon: Option<Icon>,
    gap_before: bool,
}

pub struct Toolbar {
    buttons: Vec<Button>,
    pub load_note: String,
}

impl Toolbar {
    pub fn load() -> Self {
        let mut load_note = String::new();
        let icons = match Imaging::new() {
            Ok(factory) => {
                let mut loaded = Vec::new();
                for &(command, file, gap) in BUTTONS {
                    if file.is_empty() {
                        loaded.push(Button { command, icon: None, gap_before: gap });
                        continue;
                    }
                    match factory.decode(&icon_path(file)) {
                        Ok(icon) => loaded.push(Button { command, icon: Some(icon), gap_before: gap }),
                        Err(error) => {
                            if !load_note.is_empty() {
                                load_note.push(' ');
                            }
                            load_note.push_str(&format!("{file}: {error}"));
                            loaded.push(Button { command, icon: None, gap_before: gap });
                        }
                    }
                }
                loaded
            }
            Err(error) => {
                load_note = error;
                BUTTONS.iter().map(|(command, _, gap)| Button { command: *command, icon: None, gap_before: *gap }).collect()
            }
        };
        Self { buttons: icons, load_note }
    }

    pub fn command(&self, index: usize) -> Option<ToolbarCommand> {
        self.buttons.get(index).map(|button| button.command)
    }

    pub fn hit(&self, dpi: u32, x: i32, y: i32, show_modeling: bool) -> Option<usize> {
        self.slots(dpi, show_modeling)
            .into_iter()
            .find(|slot| x >= slot.x && y >= slot.y && x < slot.x + slot.width && y < slot.y + slot.height)
            .map(|slot| slot.index)
    }

    pub fn paint(
        &self,
        hdc: HDC,
        width: i32,
        height: i32,
        dpi: u32,
        hot: Option<usize>,
        active: ToolbarCommand,
        local_space: bool,
        session: Option<ToolbarCommand>,
        font: HFONT,
        mode: WorkspaceMode,
        show_modeling: bool,
        modeling: Option<ToolbarCommand>,
    ) {
        unsafe {
            let rect = RECT { left: 0, top: 0, right: width, bottom: height };
            FillRect(hdc, &rect, chrome_brush());
            let line = RECT { left: 0, top: height - 1, right: width, bottom: height };
            FillRect(hdc, &line, brush(brushes().line));
            if !font.is_null() {
                SelectObject(hdc, font);
            }
            SetBkMode(hdc, 1);
        }
        for slot in self.slots(dpi, show_modeling) {
            let button = &self.buttons[slot.index];
            let disabled = button.command == ToolbarCommand::Scale;
            let on = !disabled
                && ((button.command.is_object_tool() && button.command == active)
                    || (button.command == ToolbarCommand::Local && local_space)
                    || (button.command == ToolbarCommand::Level && mode == WorkspaceMode::Level)
                    || (button.command == ToolbarCommand::Land && mode == WorkspaceMode::Land)
                    || (button.command == ToolbarCommand::Character && mode == WorkspaceMode::Character)
                    || session == Some(button.command)
                    || modeling == Some(button.command));
            let hot = hot == Some(slot.index);
            let fill = if disabled {
                brush(brushes().button)
            } else if on {
                brush(brushes().button_on)
            } else if hot {
                brush(brushes().button_hot)
            } else {
                brush(brushes().button)
            };
            let tone = if disabled {
                IconTone::Dim
            } else if button.command.is_object_tool() {
                if on {
                    IconTone::Active
                } else if hot {
                    IconTone::Hot
                } else {
                    IconTone::Muted
                }
            } else {
                IconTone::Full
            };
            let background = if on { BUTTON_ON } else if hot { BUTTON_HOT } else { BUTTON };
            let rect = RECT { left: slot.x, top: slot.y, right: slot.x + slot.width, bottom: slot.y + slot.height };
            unsafe { FillRect(hdc, &rect, fill); }
            if on {
                let accent = RECT {
                    left: slot.x,
                    top: slot.y + slot.height - 2,
                    right: slot.x + slot.width,
                    bottom: slot.y + slot.height,
                };
                unsafe { FillRect(hdc, &accent, brush(brushes().accent)); }
            }
            let icon_x = slot.x + (slot.width - slot.icon).max(0) / 2;
            let icon_y = slot.y + slot.pad_top;
            if let Some(icon) = &button.icon {
                blit_icon(hdc, icon, icon_x, icon_y, slot.icon, background, tone);
            }
            let caption = wide(button.command.caption_for(local_space));
            let mut extent = SIZE { cx: 0, cy: 0 };
            let caption_color = if disabled {
                rgb(96, 96, 96)
            } else if on {
                TEXT
            } else if button.command.is_object_tool() || button.command.is_workspace() {
                TEXT_DIM
            } else {
                TEXT
            };
            unsafe {
                GetTextExtentPoint32W(hdc, caption.as_ptr(), (caption.len() - 1) as i32, &mut extent);
                let text_x = slot.x + (slot.width - extent.cx).max(0) / 2;
                let text_y = slot.y + slot.height - slot.pad_bottom - extent.cy;
                SetTextColor(hdc, caption_color);
                TextOutW(hdc, text_x, text_y, caption.as_ptr(), (caption.len() - 1) as i32);
            }
            if slot.gap {
                let separator = RECT {
                    left: slot.x - slot.gap_width / 2,
                    top: slot.y + 6,
                    right: slot.x - slot.gap_width / 2 + 1,
                    bottom: slot.y + slot.height - 6,
                };
                unsafe { FillRect(hdc, &separator, brush(brushes().line)); }
            }
        }
    }

    /// One inspector shelf button. Tiles put the toolbar icon over the caption.
    pub fn paint_shelf(&self, hdc: HDC, rect: RECT, caption: &str, font: HFONT, icon: Option<ToolbarCommand>, on: bool, pressed: bool, kind: ShelfPaint) {
        let background = if on { BUTTON_ON } else if pressed { BUTTON_HOT } else { BUTTON };
        let fill = if on {
            brush(brushes().button_on)
        } else if pressed {
            brush(brushes().button_hot)
        } else {
            brush(brushes().button)
        };
        unsafe {
            FillRect(hdc, &rect, fill);
            if on {
                let accent = if kind == ShelfPaint::Row {
                    RECT { left: rect.left, top: rect.top, right: rect.left + 2, bottom: rect.bottom }
                } else {
                    RECT { left: rect.left, top: rect.bottom - 2, right: rect.right, bottom: rect.bottom }
                };
                FillRect(hdc, &accent, brush(brushes().accent));
            }
            if !font.is_null() {
                SelectObject(hdc, font);
            }
            SetBkMode(hdc, 1);
            SetTextColor(hdc, if on { TEXT } else { TEXT_DIM });
        }
        if kind == ShelfPaint::Tile {
            if let Some(command) = icon {
                if let Some(icon) = self.buttons.iter().find(|button| button.command == command).and_then(|button| button.icon.as_ref()) {
                    let size = ((rect.bottom - rect.top) - 22).clamp(16, 26);
                    let x = rect.left + ((rect.right - rect.left) - size).max(0) / 2;
                    let y = rect.top + 4;
                    let tone = if on { IconTone::Active } else { IconTone::Muted };
                    blit_icon(hdc, icon, x, y, size, background, tone);
                }
            }
        }
        let caption = wide(caption);
        let mut extent = SIZE { cx: 0, cy: 0 };
        unsafe {
            GetTextExtentPoint32W(hdc, caption.as_ptr(), (caption.len() - 1) as i32, &mut extent);
            let (text_x, text_y) = match kind {
                ShelfPaint::Row => (rect.left + 10, rect.top + ((rect.bottom - rect.top) - extent.cy).max(0) / 2),
                ShelfPaint::Tile => (rect.left + ((rect.right - rect.left) - extent.cx).max(0) / 2, rect.bottom - 4 - extent.cy),
                ShelfPaint::Chip => (
                    rect.left + ((rect.right - rect.left) - extent.cx).max(0) / 2,
                    rect.top + ((rect.bottom - rect.top) - extent.cy).max(0) / 2,
                ),
            };
            TextOutW(hdc, text_x, text_y, caption.as_ptr(), (caption.len() - 1) as i32);
        }
    }

    fn slots(&self, dpi: u32, show_modeling: bool) -> Vec<Slot> {
        let metrics = toolbar_metrics(dpi);
        let mut x = dip(6.0, dpi);
        let mut slots = Vec::with_capacity(self.buttons.len());
        for (index, button) in self.buttons.iter().enumerate() {
            if button.command.is_modeling() && !show_modeling {
                continue;
            }
            let gap_width = if button.gap_before { metrics.group_gap } else { metrics.item_gap };
            if index > 0 {
                x += gap_width;
            }
            // "World" is wider than "Local" because of W. The tile stays that wide either way.
            let text = if button.command == ToolbarCommand::Local { "World" } else { button.command.caption() };
            let width = dip(14.0 + text.chars().count() as f32 * 6.6, dpi).max(metrics.min_width);
            slots.push(Slot {
                index,
                x,
                y: metrics.y,
                height: metrics.height,
                width,
                icon: metrics.icon,
                pad_top: metrics.pad_top,
                pad_bottom: metrics.pad_bottom,
                gap: button.gap_before && index > 0,
                gap_width,
            });
            x += width;
        }
        slots
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShelfPaint {
    Chip,
    Tile,
    Row,
}

struct Slot {
    index: usize,
    x: i32,
    y: i32,
    height: i32,
    width: i32,
    icon: i32,
    pad_top: i32,
    pad_bottom: i32,
    gap: bool,
    gap_width: i32,
}

struct ToolbarMetrics {
    y: i32,
    height: i32,
    icon: i32,
    pad_top: i32,
    pad_bottom: i32,
    item_gap: i32,
    group_gap: i32,
    min_width: i32,
}

fn toolbar_metrics(dpi: u32) -> ToolbarMetrics {
    ToolbarMetrics {
        y: dip(3.0, dpi),
        height: dip(50.0, dpi).max(40),
        icon: dip(26.0, dpi).max(18),
        pad_top: dip(3.0, dpi).max(2),
        pad_bottom: dip(4.0, dpi).max(3),
        item_gap: dip(3.0, dpi).max(2),
        group_gap: dip(10.0, dpi).max(6),
        min_width: dip(40.0, dpi).max(32),
    }
}

const BUTTONS: &[(ToolbarCommand, &str, bool)] = &[
    (ToolbarCommand::Select, "select_cursor.png", false),
    (ToolbarCommand::Translate, "translate.png", false),
    (ToolbarCommand::Rotate, "rotate.png", false),
    (ToolbarCommand::Scale, "scale.png", false),
    (ToolbarCommand::Block, "block.png", false),
    (ToolbarCommand::Extrude, "extrude.png", true),
    (ToolbarCommand::Inset, "inset.png", false),
    (ToolbarCommand::Bevel, "bevel.png", false),
    (ToolbarCommand::Subdivide, "subdivide.png", false),
    (ToolbarCommand::MoveEdge, "move_edge.png", false),
    (ToolbarCommand::ExtrudeEdge, "extrude_edge.png", false),
    (ToolbarCommand::SplitEdge, "split_edge.png", false),
    (ToolbarCommand::MoveVertex, "move_vertex.png", false),
    (ToolbarCommand::Level, "level.png", true),
    (ToolbarCommand::Land, "land.png", false),
    (ToolbarCommand::Character, "character.png", false),
    (ToolbarCommand::Play, "play.png", true),
    (ToolbarCommand::Pause, "pause.png", false),
    (ToolbarCommand::Stop, "stop.png", false),
    (ToolbarCommand::Fly, "camera_fly.png", true),
    (ToolbarCommand::Pan, "pan.png", false),
    (ToolbarCommand::Orbit, "orbit.png", false),
    (ToolbarCommand::Focus, "focus_selected.png", false),
    (ToolbarCommand::Snap, "grid_snap.png", true),
    (ToolbarCommand::Local, "local_world.png", false),
    (ToolbarCommand::Speed, "speed.png", false),
    (ToolbarCommand::Maximize, "maximize_viewport.png", false),
];

fn icon_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/editor/ui/navigation").join(file)
}

fn dip(value: f32, dpi: u32) -> i32 {
    (value * dpi.max(1) as f32 / 96.0).round() as i32
}

#[derive(Clone, Copy)]
enum IconTone {
    Full,
    Muted,
    Hot,
    Active,
    Dim,
}

fn blit_icon(hdc: HDC, icon: &Icon, x: i32, y: i32, size: i32, background: u32, tone: IconTone) {
    if size <= 0 {
        return;
    }
    let size = size as u32;
    let mut pixels = scale_premultiplied(icon, size);
    recolor(&mut pixels, tone);
    let bg_b = (background & 0xff) as u8;
    let bg_g = ((background >> 8) & 0xff) as u8;
    let bg_r = ((background >> 16) & 0xff) as u8;
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = pixel[3] as u32;
        let inverse = 255 - alpha;
        pixel[0] = (pixel[0] as u32 + bg_b as u32 * inverse / 255) as u8;
        pixel[1] = (pixel[1] as u32 + bg_g as u32 * inverse / 255) as u8;
        pixel[2] = (pixel[2] as u32 + bg_r as u32 * inverse / 255) as u8;
        pixel[3] = 255;
    }
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size as i32,
            biHeight: -(size as i32),
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
    unsafe {
        SetDIBitsToDevice(hdc, x, y, size, size, 0, 0, 0, size, pixels.as_ptr().cast(), &info, DIB_RGB_COLORS);
    }
}

fn recolor(pixels: &mut [u8], tone: IconTone) {
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = pixel[3] as u32;
        if alpha == 0 {
            continue;
        }
        let (blue, green, red) = match tone {
            IconTone::Full => continue,
            IconTone::Muted => mix_gray(pixel, alpha, 180),
            IconTone::Hot => mix_gray(pixel, alpha, 210),
            IconTone::Active => {
                let blue = mix(pixel[0], 255, alpha);
                let green = mix(pixel[1], 180, alpha);
                let red = mix(pixel[2], 74, alpha);
                (blue, green, red)
            }
            IconTone::Dim => mix_gray(pixel, alpha, 70),
        };
        pixel[0] = blue;
        pixel[1] = green;
        pixel[2] = red;
    }
}

fn mix_gray(pixel: &[u8], alpha: u32, level: u32) -> (u8, u8, u8) {
    let luma = (pixel[0] as u32 + pixel[1] as u32 + pixel[2] as u32) / 3;
    let gray = luma * level / 255;
    (mix_channel(pixel[0], gray, alpha), mix_channel(pixel[1], gray, alpha), mix_channel(pixel[2], gray, alpha))
}

fn mix(channel: u8, target: u32, alpha: u32) -> u8 {
    mix_channel(channel, target * alpha / 255, alpha)
}

fn mix_channel(channel: u8, toward: u32, _alpha: u32) -> u8 {
    ((channel as u32 + toward) / 2) as u8
}

fn scale_premultiplied(icon: &Icon, size: u32) -> Vec<u8> {
    let mut out = vec![0u8; (size * size * 4) as usize];
    if icon.width == 0 || icon.height == 0 {
        return out;
    }
    for y in 0..size {
        let y0 = y * icon.height / size;
        let y1 = ((y + 1) * icon.height / size).max(y0 + 1).min(icon.height);
        for x in 0..size {
            let x0 = x * icon.width / size;
            let x1 = ((x + 1) * icon.width / size).max(x0 + 1).min(icon.width);
            let mut sum = [0u32; 4];
            let mut count = 0u32;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let index = ((sy * icon.width + sx) * 4) as usize;
                    for channel in 0..4 {
                        sum[channel] += icon.pixels[index + channel] as u32;
                    }
                    count += 1;
                }
            }
            let dest = ((y * size + x) * 4) as usize;
            if count > 0 {
                for channel in 0..4 {
                    out[dest + channel] = (sum[channel] / count) as u8;
                }
            }
        }
    }
    out
}

struct Imaging {
    factory: *mut std::ffi::c_void,
}

impl Imaging {
    fn new() -> Result<Self, String> {
        unsafe {
            let _ = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
            let mut factory = std::ptr::null_mut();
            let hr = CoCreateInstance(
                &guid(0xcacaf262_9370_4615_a13b_9f5539da4c0a),
                std::ptr::null_mut(),
                CLSCTX_INPROC_SERVER,
                &guid(0xec5ec8a9_c395_4314_9c77_54d7a935ff70),
                &mut factory,
            );
            if hr < 0 || factory.is_null() {
                return Err(format!("imaging factory {hr:#x}"));
            }
            Ok(Self { factory })
        }
    }

    fn decode(&self, path: &std::path::Path) -> Result<Icon, String> {
        let name = wide(path.to_string_lossy().as_ref());
        unsafe {
            let open: CreateDecoderFromFilename = vslot(self.factory, 3);
            let mut decoder = std::ptr::null_mut();
            let hr = open(self.factory, name.as_ptr(), std::ptr::null(), GENERIC_READ, 0, &mut decoder);
            if hr < 0 || decoder.is_null() {
                return Err(format!("icon open {hr:#x}"));
            }
            let get_frame: GetFrame = vslot(decoder, 13);
            let mut frame = std::ptr::null_mut();
            let hr = get_frame(decoder, 0, &mut frame);
            if hr < 0 || frame.is_null() {
                release(decoder);
                return Err(format!("icon frame {hr:#x}"));
            }
            let make: CreateConverter = vslot(self.factory, 10);
            let mut converter = std::ptr::null_mut();
            let hr = make(self.factory, &mut converter);
            if hr < 0 || converter.is_null() {
                release(frame);
                release(decoder);
                return Err(format!("icon converter {hr:#x}"));
            }
            let format = guid(0x6fddc324_4e03_4bfe_b185_3d77768dc90d);
            let init: Initialize = vslot(converter, 8);
            let hr = init(converter, frame, &format, 0, std::ptr::null_mut(), 0.0, 0);
            if hr < 0 {
                release(converter);
                release(frame);
                release(decoder);
                return Err(format!("icon convert {hr:#x}"));
            }
            let mut width = 0u32;
            let mut height = 0u32;
            let get_size: GetSize = vslot(converter, 3);
            let hr = get_size(converter, &mut width, &mut height);
            if hr < 0 || width == 0 || height == 0 || width > 4096 || height > 4096 {
                release(converter);
                release(frame);
                release(decoder);
                return Err(format!("icon size {width}x{height} {hr:#x}"));
            }
            let stride = width.saturating_mul(4);
            let mut pixels = vec![0u8; (stride as usize).saturating_mul(height as usize)];
            let copy: CopyPixels = vslot(converter, 7);
            let hr = copy(converter, std::ptr::null(), stride, pixels.len() as u32, pixels.as_mut_ptr());
            release(converter);
            release(frame);
            release(decoder);
            if hr < 0 {
                return Err(format!("icon pixels {hr:#x}"));
            }
            Ok(Icon { width, height, pixels })
        }
    }
}

impl Drop for Imaging {
    fn drop(&mut self) {
        release(self.factory);
    }
}

type CreateDecoderFromFilename = unsafe extern "system" fn(
    *mut std::ffi::c_void,
    *const u16,
    *const GUID,
    u32,
    u32,
    *mut *mut std::ffi::c_void,
) -> i32;
type GetFrame = unsafe extern "system" fn(*mut std::ffi::c_void, u32, *mut *mut std::ffi::c_void) -> i32;
type CreateConverter = unsafe extern "system" fn(*mut std::ffi::c_void, *mut *mut std::ffi::c_void) -> i32;
type Initialize = unsafe extern "system" fn(
    *mut std::ffi::c_void,
    *mut std::ffi::c_void,
    *const GUID,
    u32,
    *mut std::ffi::c_void,
    f64,
    u32,
) -> i32;
type GetSize = unsafe extern "system" fn(*mut std::ffi::c_void, *mut u32, *mut u32) -> i32;
type CopyPixels = unsafe extern "system" fn(*mut std::ffi::c_void, *const u8, u32, u32, *mut u8) -> i32;

fn vslot<T>(object: *mut std::ffi::c_void, slot: usize) -> T {
    unsafe {
        let table = *(object as *const *const usize);
        std::mem::transmute_copy(&*table.add(slot))
    }
}

fn release(object: *mut std::ffi::c_void) {
    if object.is_null() {
        return;
    }
    type Release = unsafe extern "system" fn(*mut std::ffi::c_void) -> u32;
    let release: Release = vslot(object, 2);
    unsafe { release(object); }
}

fn guid(value: u128) -> GUID {
    GUID::from_u128(value)
}

fn proc_ordinal(module: windows_sys::Win32::Foundation::HMODULE, ordinal: usize) -> windows_sys::Win32::Foundation::FARPROC {
    unsafe { GetProcAddress(module, ordinal as *const u8) }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn menu_labels() -> &'static std::sync::Mutex<Vec<String>> {
    static LABELS: std::sync::OnceLock<std::sync::Mutex<Vec<String>>> = std::sync::OnceLock::new();
    LABELS.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

pub fn darken_menu(menu: windows_sys::Win32::UI::WindowsAndMessaging::HMENU) {
    let mut info: MENUINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MENUINFO>() as u32;
    info.fMask = MIM_BACKGROUND | MIM_APPLYTOSUBMENUS;
    info.hbrBack = chrome_brush();
    unsafe {
        let _ = SetMenuInfo(menu, &info);
    }
}

pub fn own_menu_item(menu: windows_sys::Win32::UI::WindowsAndMessaging::HMENU, position: u32, label: &str) {
    let index = {
        let mut labels = menu_labels().lock().unwrap_or_else(|poison| poison.into_inner());
        labels.push(label.to_string());
        labels.len() - 1
    };
    let mut info: MENUITEMINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
    info.fMask = MIIM_FTYPE | MIIM_DATA;
    info.fType = MFT_OWNERDRAW;
    info.dwItemData = index;
    unsafe {
        let _ = SetMenuItemInfoW(menu, position, 1, &info);
    }
}

pub fn measure_menu(item: &mut MEASUREITEMSTRUCT) {
    if item.CtlType != ODT_MENU {
        return;
    }
    let label = menu_text(item.itemData);
    let text = wide(&label);
    let mut size = SIZE { cx: 0, cy: 0 };
    unsafe {
        let dc = windows_sys::Win32::Graphics::Gdi::GetDC(std::ptr::null_mut());
        if !dc.is_null() {
            let _ = GetTextExtentPoint32W(dc, text.as_ptr(), (text.len() - 1) as i32, &mut size);
            windows_sys::Win32::Graphics::Gdi::ReleaseDC(std::ptr::null_mut(), dc);
        }
    }
    item.itemWidth = (size.cx as u32).saturating_add(36);
    item.itemHeight = (size.cy as u32).saturating_add(10).max(24);
}

pub fn paint_menu(item: &DRAWITEMSTRUCT, font: HFONT) {
    if item.CtlType != ODT_MENU {
        return;
    }
    let selected = item.itemState & ODS_SELECTED != 0;
    let checked = item.itemState & ODS_CHECKED != 0;
    let label = menu_text(item.itemData);
    let text = wide(&label);
    unsafe {
        let fill = if selected { brush(brushes().button_hot) } else { chrome_brush() };
        FillRect(item.hDC, &item.rcItem, fill);
        if checked {
            let mark = RECT {
                left: item.rcItem.left + 8,
                top: item.rcItem.top + 8,
                right: item.rcItem.left + 14,
                bottom: item.rcItem.bottom - 8,
            };
            FillRect(item.hDC, &mark, brush(brushes().accent));
        }
        if !font.is_null() {
            SelectObject(item.hDC, font);
        }
        SetBkMode(item.hDC, 1);
        SetTextColor(item.hDC, TEXT);
        TextOutW(item.hDC, item.rcItem.left + 22, item.rcItem.top + 4, text.as_ptr(), (text.len() - 1) as i32);
    }
}

fn menu_text(index: usize) -> String {
    menu_labels().lock().unwrap_or_else(|poison| poison.into_inner()).get(index).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_icons_decode_with_transparency() {
        let toolbar = Toolbar::load();
        assert!(toolbar.load_note.is_empty(), "{}", toolbar.load_note);
        assert_eq!(toolbar.buttons.len(), BUTTONS.len());
        assert!(ToolbarCommand::Extrude.is_modeling());
        assert!(ToolbarCommand::Subdivide.is_modeling());
        assert!(ToolbarCommand::MoveEdge.is_modeling());
        assert!(ToolbarCommand::ExtrudeEdge.is_modeling());
        assert!(ToolbarCommand::SplitEdge.is_modeling());
        assert!(ToolbarCommand::MoveVertex.is_modeling());
        assert!(!ToolbarCommand::Block.is_modeling());
        assert!(!ToolbarCommand::Block.is_object_tool());
        assert!(ToolbarCommand::Translate.is_object_tool());
        assert!(ToolbarCommand::Level.is_workspace());
        assert_eq!(ToolbarCommand::Translate.caption(), "Move");
        assert_eq!(ToolbarCommand::Translate.label(), "Translate");
        assert_eq!(ToolbarCommand::Local.caption_for(true), "Local");
        assert_eq!(ToolbarCommand::Local.caption_for(false), "World");
        for button in &toolbar.buttons {
            assert!(!button.command.caption().is_empty(), "{:?}", button.command);
            let icon = button.icon.as_ref().unwrap_or_else(|| panic!("missing icon for {:?}", button.command));
            assert_eq!((icon.width, icon.height), (128, 128), "{:?}", button.command);
            assert_eq!(icon.pixels[3], 0, "corner alpha {:?}", button.command);
            assert!(icon.pixels.chunks(4).any(|pixel| pixel[3] > 200), "{:?}", button.command);
        }
        let slots = toolbar.slots(96, true);
        assert_eq!(slots.len(), BUTTONS.len());
        assert_eq!(toolbar.slots(96, false).len(), BUTTONS.len() - 8);
        for slot in &slots {
            assert!(slot.height > slot.icon, "caption band");
            assert!(slot.width >= slot.icon);
        }
        let hit = toolbar.hit(96, slots[0].x + 2, slots[0].y + slots[0].height - 3, true);
        assert_eq!(hit, Some(0));
        let above = toolbar.hit(96, slots[0].x + 2, slots[0].y - 2, true);
        assert_eq!(above, None);
    }
}


