//! Win32 project browser. It parses a project manifest and lists recent files.
//!
//! It does not construct `EngineSession`, open Lighting Lab, scan a content
//! tree, decode a thumbnail, or upload a texture. Splash progress belongs to
//! the editor, after a project has been chosen.

use std::path::{Path, PathBuf};

use jarvig_core::{
    create_project_at, hub_recent_file, load_project_file, load_recent_at, locate_recent_at, project_file_missing, remember_recent_at,
    remove_recent_at, ProjectTemplate, RecentList,
};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, GetStockObject, SetBkColor, SetBkMode, SetTextColor, CLEARTYPE_QUALITY, DEFAULT_CHARSET, DEFAULT_GUI_FONT,
    FW_NORMAL, HBRUSH, HDC, TRANSPARENT,
};
use windows_sys::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_APARTMENTTHREADED};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OPENFILENAMEW, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST,
};
use windows_sys::Win32::UI::Controls::SetWindowTheme;
use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows_sys::Win32::UI::Shell::{SHBrowseForFolderW, SHGetPathFromIDListW, BIF_EDITBOX, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW, GetWindowLongPtrW, GetWindowTextLengthW,
    GetWindowTextW, IsDialogMessageW, LoadCursorW, PostQuitMessage, RegisterClassW, SendMessageW, SetProcessDPIAware, SetWindowLongPtrW,
    SetWindowPos, SetWindowTextW,
    ShowWindow, TranslateMessage, CREATESTRUCTW, CW_USEDEFAULT, ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE, ES_READONLY, GWLP_USERDATA,
    HMENU, IDC_ARROW, LBS_NOINTEGRALHEIGHT, LBS_NOTIFY, MSG, SW_HIDE, SW_SHOW, SWP_NOZORDER, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_CTLCOLORBTN,
    WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_CTLCOLORSTATIC, WM_DESTROY, WM_NCCREATE, WM_SIZE, WNDCLASSW, WS_BORDER, WS_CHILD, WS_GROUP,
    WS_OVERLAPPEDWINDOW, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};

const CHROME: u32 = rgb(24, 24, 24);
const PANEL: u32 = rgb(36, 36, 36);
const FIELD: u32 = rgb(22, 22, 22);
const TEXT: u32 = rgb(220, 220, 220);

const ID_NEW: isize = 100;
const ID_OPEN: isize = 101;
const ID_OPEN_SEL: isize = 102;
const ID_REMOVE: isize = 103;
const ID_LOCATE: isize = 104;
const ID_BACK: isize = 105;
const ID_BROWSE: isize = 106;
const ID_CREATE: isize = 107;
const ID_LIST: isize = 108;
const ID_NAME: isize = 109;
const ID_LOCATION: isize = 110;
const ID_BLANK: isize = 120;
const ID_FIRST: isize = 121;
const ID_THIRD: isize = 122;
const ID_LAND: isize = 123;

const LB_ADDSTRING: u32 = 0x0180;
const LB_RESETCONTENT: u32 = 0x0184;
const LB_SETCURSEL: u32 = 0x0186;
const LB_GETCURSEL: u32 = 0x0188;
const BM_SETCHECK: u32 = 0x00F1;
const WM_SETFONT: u32 = 0x0030;
const LBN_SELCHANGE: usize = 1;
const LBN_DBLCLK: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Recent,
    New,
}

struct Hub {
    hwnd: HWND,
    page: Page,
    recents: RecentList,
    store: PathBuf,
    selected: isize,
    template: ProjectTemplate,
    chosen: Option<PathBuf>,
    font: isize,
    title: HWND,
    list: HWND,
    detail: HWND,
    status: HWND,
    new_btn: HWND,
    open_btn: HWND,
    open_sel_btn: HWND,
    remove_btn: HWND,
    locate_btn: HWND,
    summary: HWND,
    name_label: HWND,
    name_edit: HWND,
    location_label: HWND,
    location_edit: HWND,
    browse_btn: HWND,
    create_btn: HWND,
    back_btn: HWND,
    templates: [HWND; 4],
}

/// Recent projects, New Project, and Open Project. `Ok(None)` is the user closing the window.
pub fn pick_project() -> Result<Option<PathBuf>, String> {
    unsafe { run_hub() }
}

unsafe fn run_hub() -> Result<Option<PathBuf>, String> {
    let _ = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
    if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) == 0 {
        let _ = SetProcessDPIAware();
    }
    enable_dark_app();
    let instance = GetModuleHandleW(std::ptr::null());
    let class_name = wide("JARVIGHubWindow");
    let class = WNDCLASSW {
        lpfnWndProc: Some(hub_proc),
        hInstance: instance,
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        hbrBackground: CreateSolidBrush(CHROME),
        lpszClassName: class_name.as_ptr(),
        ..std::mem::zeroed()
    };
    if RegisterClassW(&class) == 0 && windows_sys::Win32::Foundation::GetLastError() != 1410 {
        return Err("JARVIG hub window class was not registered".into());
    }
    let store = hub_recent_file();
    let recents = load_recent_at(&store).unwrap_or_else(|_| RecentList { projects: Vec::new() });
    let hub = Box::new(Hub {
        hwnd: std::ptr::null_mut(),
        page: Page::Recent,
        recents,
        store,
        selected: -1,
        template: ProjectTemplate::Blank,
        chosen: None,
        font: 0,
        title: std::ptr::null_mut(),
        list: std::ptr::null_mut(),
        detail: std::ptr::null_mut(),
        status: std::ptr::null_mut(),
        new_btn: std::ptr::null_mut(),
        open_btn: std::ptr::null_mut(),
        open_sel_btn: std::ptr::null_mut(),
        remove_btn: std::ptr::null_mut(),
        locate_btn: std::ptr::null_mut(),
        summary: std::ptr::null_mut(),
        name_label: std::ptr::null_mut(),
        name_edit: std::ptr::null_mut(),
        location_label: std::ptr::null_mut(),
        location_edit: std::ptr::null_mut(),
        browse_btn: std::ptr::null_mut(),
        create_btn: std::ptr::null_mut(),
        back_btn: std::ptr::null_mut(),
        templates: [std::ptr::null_mut(); 4],
    });
    let raw = Box::into_raw(hub);
    let title = wide("JARVIG");
    let hwnd = CreateWindowExW(
        0,
        class_name.as_ptr(),
        title.as_ptr(),
        WS_OVERLAPPEDWINDOW,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        960,
        640,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        instance,
        raw.cast(),
    );
    if hwnd.is_null() {
        drop(Box::from_raw(raw));
        return Err("JARVIG hub window was not created".into());
    }
    ShowWindow(hwnd, SW_SHOW);
    let mut message: MSG = std::mem::zeroed();
    while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
        if IsDialogMessageW(hwnd, &message) == 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    let hub = Box::from_raw(raw);
    Ok(hub.chosen)
}

unsafe extern "system" fn hub_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_NCCREATE {
        let create = lparam as *const CREATESTRUCTW;
        let hub = (*create).lpCreateParams as *mut Hub;
        if !hub.is_null() {
            (*hub).hwnd = hwnd;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, hub as isize);
        }
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let hub = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Hub;
    if hub.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let hub = &mut *hub;
    match message {
        WM_CREATE => {
            hub.build_children();
            hub.refresh_list();
            hub.show_page(Page::Recent);
            hub.layout();
            0
        }
        WM_SIZE => {
            hub.layout();
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xffff) as isize;
            let note = (wparam >> 16) & 0xffff;
            if id == ID_LIST && note as usize == LBN_SELCHANGE {
                hub.selected = SendMessageW(hub.list, LB_GETCURSEL, 0, 0) as isize;
                hub.show_detail();
            } else if id == ID_LIST && note as usize == LBN_DBLCLK {
                hub.open_selected();
            } else if note == 0 {
                hub.on_click(id);
            }
            0
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORBTN => {
            let dc = wparam as HDC;
            SetBkMode(dc, TRANSPARENT as i32);
            SetTextColor(dc, TEXT);
            SetBkColor(dc, if message == WM_CTLCOLOREDIT || message == WM_CTLCOLORLISTBOX { FIELD } else { PANEL });
            if message == WM_CTLCOLOREDIT || message == WM_CTLCOLORLISTBOX {
                field_brush() as LRESULT
            } else {
                panel_brush() as LRESULT
            }
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

impl Hub {
    unsafe fn build_children(&mut self) {
        enable_dark_caption(self.hwnd);
        self.font = CreateFontW(
            -18,
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
            wide("Segoe UI").as_ptr(),
        ) as isize;
        if self.font == 0 {
            self.font = GetStockObject(DEFAULT_GUI_FONT) as isize;
        }
        self.title = self.child("STATIC", "JARVIG", 0, 0);
        self.list = self.child(
            "LISTBOX",
            "",
            WS_TABSTOP | WS_VSCROLL | WS_BORDER | LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32,
            ID_LIST,
        );
        self.detail = self.child("EDIT", "", WS_VSCROLL | WS_BORDER | ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32, 0);
        self.status = self.child("STATIC", "Choose a project. Nothing is loaded yet.", 0, 0);
        self.new_btn = self.child("BUTTON", "New Project", WS_TABSTOP, ID_NEW);
        self.open_btn = self.child("BUTTON", "Open Project", WS_TABSTOP, ID_OPEN);
        self.open_sel_btn = self.child("BUTTON", "Open", WS_TABSTOP, ID_OPEN_SEL);
        self.remove_btn = self.child("BUTTON", "Remove", WS_TABSTOP, ID_REMOVE);
        self.locate_btn = self.child("BUTTON", "Locate", WS_TABSTOP, ID_LOCATE);
        self.summary = self.child("STATIC", ProjectTemplate::Blank.summary(), 0, 0);
        let labels = [
            (ID_BLANK, ProjectTemplate::Blank),
            (ID_FIRST, ProjectTemplate::FirstPerson),
            (ID_THIRD, ProjectTemplate::ThirdPerson),
            (ID_LAND, ProjectTemplate::Landscape),
        ];
        for (index, (id, template)) in labels.into_iter().enumerate() {
            let style = WS_TABSTOP | if index == 0 { WS_GROUP } else { 0u32 };
            self.templates[index] = self.child("BUTTON", template.label(), style | 0x0009, id);
        }
        SendMessageW(self.templates[0], BM_SETCHECK, 1, 0);
        self.name_label = self.child("STATIC", "Project Name", 0, 0);
        self.name_edit = self.child("EDIT", "MyGame", WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32, ID_NAME);
        self.location_label = self.child("STATIC", "Project Location", 0, 0);
        self.location_edit = self.child("EDIT", &default_location(), WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32, ID_LOCATION);
        self.browse_btn = self.child("BUTTON", "Browse", WS_TABSTOP, ID_BROWSE);
        self.create_btn = self.child("BUTTON", "Create", WS_TABSTOP, ID_CREATE);
        self.back_btn = self.child("BUTTON", "Back", WS_TABSTOP, ID_BACK);
    }

    unsafe fn child(&self, class: &str, text: &str, style: u32, id: isize) -> HWND {
        let class = wide(class);
        let text = wide(text);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            text.as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            10,
            10,
            self.hwnd,
            id as HMENU,
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        SendMessageW(hwnd, WM_SETFONT, self.font as usize, 1);
        let theme = wide("DarkMode_Explorer");
        SetWindowTheme(hwnd, theme.as_ptr(), std::ptr::null());
        hwnd
    }

    unsafe fn on_click(&mut self, id: isize) {
        match id {
            ID_NEW => self.show_page(Page::New),
            ID_BACK => self.show_page(Page::Recent),
            ID_OPEN => {
                if let Some(path) = open_project_dialog(self.hwnd) {
                    self.accept(path);
                }
            }
            ID_OPEN_SEL => self.open_selected(),
            ID_REMOVE => self.remove_selected(),
            ID_LOCATE => self.locate_selected(),
            ID_BROWSE => {
                if let Some(path) = browse_folder(self.hwnd) {
                    set_text(self.location_edit, &path.display().to_string());
                }
            }
            ID_CREATE => self.create_project(),
            ID_BLANK => self.pick_template(ProjectTemplate::Blank),
            ID_FIRST => self.pick_template(ProjectTemplate::FirstPerson),
            ID_THIRD => self.pick_template(ProjectTemplate::ThirdPerson),
            ID_LAND => self.pick_template(ProjectTemplate::Landscape),
            _ => {}
        }
    }

    fn pick_template(&mut self, template: ProjectTemplate) {
        self.template = template;
        unsafe { set_text(self.summary, template.summary()); }
    }

    unsafe fn show_page(&mut self, page: Page) {
        self.page = page;
        let recent = page == Page::Recent;
        for hwnd in [self.list, self.detail, self.new_btn, self.open_btn, self.open_sel_btn, self.remove_btn, self.locate_btn] {
            ShowWindow(hwnd, if recent { SW_SHOW } else { SW_HIDE });
        }
        for hwnd in [
            self.summary,
            self.name_label,
            self.name_edit,
            self.location_label,
            self.location_edit,
            self.browse_btn,
            self.create_btn,
            self.back_btn,
        ] {
            ShowWindow(hwnd, if recent { SW_HIDE } else { SW_SHOW });
        }
        for hwnd in self.templates {
            ShowWindow(hwnd, if recent { SW_HIDE } else { SW_SHOW });
        }
        if recent {
            set_text(self.title, "Recent Projects");
        } else {
            set_text(self.title, "New Project");
            set_text(self.summary, self.template.summary());
        }
        self.layout();
    }

    unsafe fn layout(&self) {
        let mut rect = std::mem::zeroed();
        if GetClientRect(self.hwnd, &mut rect) == 0 {
            return;
        }
        let width = (rect.right - rect.left).max(640);
        let height = (rect.bottom - rect.top).max(480);
        place(self.title, 28, 18, width - 56, 32);
        place(self.status, 28, height - 36, width - 56, 24);
        if self.page == Page::Recent {
            place(self.list, 28, 64, width - 340, height - 160);
            place(self.detail, width - 296, 64, 268, height - 160);
            let y = height - 84;
            let buttons = [self.new_btn, self.open_btn, self.open_sel_btn, self.remove_btn, self.locate_btn];
            for (index, hwnd) in buttons.into_iter().enumerate() {
                place(hwnd, 28 + index as i32 * 132, y, 124, 32);
            }
        } else {
            for (index, hwnd) in self.templates.into_iter().enumerate() {
                let column = index as i32 % 2;
                let row = index as i32 / 2;
                place(hwnd, 28 + column * 280, 72 + row * 44, 260, 36);
            }
            place(self.summary, 28, 168, width - 56, 48);
            place(self.name_label, 28, 230, 200, 22);
            place(self.name_edit, 28, 254, width - 56, 28);
            place(self.location_label, 28, 298, 200, 22);
            place(self.location_edit, 28, 322, width - 160, 28);
            place(self.browse_btn, width - 120, 320, 92, 32);
            place(self.create_btn, 28, 376, 140, 36);
            place(self.back_btn, 180, 376, 100, 36);
        }
    }

    unsafe fn refresh_list(&mut self) {
        SendMessageW(self.list, LB_RESETCONTENT, 0, 0);
        for entry in &self.recents.projects {
            let missing = if project_file_missing(&entry.path) { "  —  Missing" } else { "" };
            let label = wide(&format!("{name}{missing}", name = entry.name));
            SendMessageW(self.list, LB_ADDSTRING, 0, label.as_ptr() as isize);
        }
        if self.selected >= 0 && (self.selected as usize) < self.recents.projects.len() {
            SendMessageW(self.list, LB_SETCURSEL, self.selected as usize, 0);
        } else {
            self.selected = -1;
        }
        self.show_detail();
    }

    unsafe fn show_detail(&self) {
        let Some(entry) = self.recents.projects.get(self.selected as usize) else {
            set_text(self.detail, "No project is selected.\r\nNew Project writes a .jarvigproject.\r\nOpen Project browses for one.");
            return;
        };
        let when = format_unix(entry.last_opened_unix);
        let thumb = if entry.thumbnail.is_empty() { "none" } else { entry.thumbnail.as_str() };
        let state = if project_file_missing(&entry.path) { "Missing" } else { "On disk" };
        set_text(
            self.detail,
            &format!(
                "{}\r\n{}\r\n\r\n{}\r\nLast opened: {}\r\nEngine: {}\r\nStartup level: {}\r\nThumbnail: {}",
                entry.name,
                state,
                entry.path.display(),
                when,
                if entry.engine_version.is_empty() { "unknown" } else { entry.engine_version.as_str() },
                if entry.startup_level.is_empty() { "unknown" } else { entry.startup_level.as_str() },
                thumb
            ),
        );
    }

    unsafe fn open_selected(&mut self) {
        let Some(path) = self.recents.projects.get(self.selected as usize).map(|entry| entry.path.clone()) else {
            self.set_status("Select a project first.");
            return;
        };
        if project_file_missing(&path) {
            self.set_status(&format!("{} is missing. It was not replaced.", path.display()));
            return;
        }
        self.accept(path);
    }

    unsafe fn remove_selected(&mut self) {
        let Some(path) = self.recents.projects.get(self.selected as usize).map(|entry| entry.path.clone()) else {
            self.set_status("Select a project first.");
            return;
        };
        match remove_recent_at(&self.store, &path) {
            Ok(list) => {
                self.recents = list;
                self.selected = -1;
                self.refresh_list();
                self.set_status("Removed from recent projects. The project files were not deleted.");
            }
            Err(error) => self.set_status(&error),
        }
    }

    unsafe fn locate_selected(&mut self) {
        let Some(from) = self.recents.projects.get(self.selected as usize).map(|entry| entry.path.clone()) else {
            self.set_status("Select a project first.");
            return;
        };
        let Some(to) = open_project_dialog(self.hwnd) else { return };
        match locate_recent_at(&self.store, &from, &to) {
            Ok(list) => {
                self.recents = list;
                self.refresh_list();
                self.set_status("That recent project now points at the file you chose.");
            }
            Err(error) => self.set_status(&error),
        }
    }

    unsafe fn accept(&mut self, path: PathBuf) {
        if project_file_missing(&path) {
            self.set_status(&format!("{} is missing. It was not replaced.", path.display()));
            return;
        }
        if load_project_file(&path).is_err() {
            self.set_status("That file is not a JARVIG project. Nothing was opened.");
            return;
        }
        if let Err(error) = remember_recent_at(&self.store, &path) {
            self.set_status(&format!("Recent list was not saved: {error}"));
        }
        self.chosen = Some(path);
        DestroyWindow(self.hwnd);
    }

    unsafe fn create_project(&mut self) {
        let name = window_text(self.name_edit);
        let name = name.trim();
        if !valid_project_name(name) {
            self.set_status("Use a project name without : / \\ or other file symbols.");
            return;
        }
        let location = PathBuf::from(window_text(self.location_edit).trim());
        if !location.is_dir() {
            self.set_status("Choose a folder that already exists.");
            return;
        }
        let folder = location.join(name);
        if folder_blocks_create(&folder) {
            self.set_status("That folder already has files. A new project was not written.");
            return;
        }
        let project_file = folder.join(format!("{name}.jarvigproject"));
        match create_project_at(&project_file, name, self.template) {
            Ok(_) => self.accept(project_file),
            Err(error) => self.set_status(&error),
        }
    }

    unsafe fn set_status(&self, text: &str) {
        set_text(self.status, text);
    }
}

fn valid_project_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 80
        && !name.ends_with('.')
        && !name.ends_with(' ')
        && !name.chars().any(|character| matches!(character, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || character.is_control())
}

fn folder_blocks_create(folder: &Path) -> bool {
    if !folder.exists() {
        return false;
    }
    match std::fs::read_dir(folder) {
        Ok(mut entries) => entries.next().is_some(),
        Err(_) => true,
    }
}

fn default_location() -> String {
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let documents = PathBuf::from(&profile).join("Documents");
        if documents.is_dir() {
            return documents.display().to_string();
        }
        if Path::new(&profile).is_dir() {
            return profile;
        }
    }
    std::env::current_dir().map(|path| path.display().to_string()).unwrap_or_else(|_| ".".into())
}

unsafe fn open_project_dialog(owner: HWND) -> Option<PathBuf> {
    let mut file = [0u16; 520];
    let filter = wide("JARVIG Project\0*.jarvigproject\0");
    let title = wide("Open JARVIG Project");
    let mut info: OPENFILENAMEW = std::mem::zeroed();
    info.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    info.hwndOwner = owner;
    info.lpstrFilter = filter.as_ptr();
    info.lpstrFile = file.as_mut_ptr();
    info.nMaxFile = file.len() as u32;
    info.lpstrTitle = title.as_ptr();
    info.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST;
    if GetOpenFileNameW(&mut info) == 0 {
        return None;
    }
    let length = file.iter().position(|unit| *unit == 0).unwrap_or(file.len());
    Some(PathBuf::from(String::from_utf16_lossy(&file[..length])))
}

unsafe fn browse_folder(owner: HWND) -> Option<PathBuf> {
    let title = wide("Project location");
    let mut display = [0u16; 260];
    let info = BROWSEINFOW {
        hwndOwner: owner,
        pidlRoot: std::ptr::null_mut(),
        pszDisplayName: display.as_mut_ptr(),
        lpszTitle: title.as_ptr(),
        ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE | BIF_EDITBOX,
        lpfn: None,
        lParam: 0,
        iImage: 0,
    };
    let pidl = SHBrowseForFolderW(&info);
    if pidl.is_null() {
        return None;
    }
    let mut path = [0u16; 520];
    let ok = SHGetPathFromIDListW(pidl, path.as_mut_ptr());
    CoTaskMemFree(pidl.cast());
    if ok == 0 {
        return None;
    }
    let length = path.iter().position(|unit| *unit == 0).unwrap_or(path.len());
    Some(PathBuf::from(String::from_utf16_lossy(&path[..length])))
}

unsafe fn place(hwnd: HWND, x: i32, y: i32, width: i32, height: i32) {
    if !hwnd.is_null() {
        SetWindowPos(hwnd, std::ptr::null_mut(), x, y, width, height, SWP_NOZORDER);
    }
}

unsafe fn set_text(hwnd: HWND, text: &str) {
    if hwnd.is_null() {
        return;
    }
    let text = wide(text);
    SetWindowTextW(hwnd, text.as_ptr());
}

unsafe fn window_text(hwnd: HWND) -> String {
    let length = GetWindowTextLengthW(hwnd).max(0) as usize;
    let mut buffer = vec![0u16; length + 1];
    let written = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) as usize;
    String::from_utf16_lossy(&buffer[..written])
}

fn format_unix(unix: i64) -> String {
    if unix <= 0 {
        return "not recorded".into();
    }
    let days = unix.div_euclid(86_400);
    let time = unix.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02} UTC", time / 3600, (time % 3600) / 60)
}

/// Days since 1970-01-01. Howard Hinnant's civil calendar.
fn civil_from_days(mut z: i64) -> (i32, u32, u32) {
    z += 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u32, day as u32)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

const fn rgb(red: u32, green: u32, blue: u32) -> u32 {
    red | (green << 8) | (blue << 16)
}

fn field_brush() -> HBRUSH {
    brush(FIELD)
}

fn panel_brush() -> HBRUSH {
    brush(PANEL)
}

fn brush(color: u32) -> HBRUSH {
    use std::sync::OnceLock;
    static FIELD_BRUSH: OnceLock<isize> = OnceLock::new();
    static PANEL_BRUSH: OnceLock<isize> = OnceLock::new();
    let slot = if color == FIELD { &FIELD_BRUSH } else { &PANEL_BRUSH };
    *slot.get_or_init(|| unsafe { CreateSolidBrush(color) as isize }) as HBRUSH
}

unsafe fn enable_dark_app() {
    let name = wide("uxtheme.dll");
    let theme = LoadLibraryW(name.as_ptr());
    if theme.is_null() {
        return;
    }
    // Ordinals 135, 104, and 136 are the undocumented dark-mode switches the editor already uses.
    if let Some(pointer) = proc_ordinal(theme, 135) {
        let set_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(pointer);
        set_mode(2);
    }
    if let Some(pointer) = proc_ordinal(theme, 104) {
        let refresh: unsafe extern "system" fn() = std::mem::transmute(pointer);
        refresh();
    }
    if let Some(pointer) = proc_ordinal(theme, 136) {
        let flush: unsafe extern "system" fn() = std::mem::transmute(pointer);
        flush();
    }
}

fn proc_ordinal(module: windows_sys::Win32::Foundation::HMODULE, ordinal: usize) -> windows_sys::Win32::Foundation::FARPROC {
    unsafe { GetProcAddress(module, ordinal as *const u8) }
}

unsafe fn enable_dark_caption(hwnd: HWND) {
    let dark = 1i32;
    let caption = CHROME;
    let text = TEXT;
    let _ = DwmSetWindowAttribute(hwnd, 20, &dark as *const i32 as *const _, 4);
    let _ = DwmSetWindowAttribute(hwnd, 19, &dark as *const i32 as *const _, 4);
    let _ = DwmSetWindowAttribute(hwnd, 35, &caption as *const u32 as *const _, 4);
    let _ = DwmSetWindowAttribute(hwnd, 36, &text as *const u32 as *const _, 4);
}

#[cfg(test)]
mod tests {
    #[test]
    fn unix_epoch_formats_as_1970() {
        assert_eq!(super::civil_from_days(0), (1970, 1, 1));
        assert_eq!(super::format_unix(0), "not recorded");
    }
}
