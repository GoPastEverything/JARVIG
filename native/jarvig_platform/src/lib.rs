//! OS window and event loop for a JARVIG host.
//!
//! `winit` implements the window. It is not a JARVIG engine type, and the
//! renderer crate does not depend on it. GPU code receives only a window handle.

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{CursorGrabMode, Window, WindowAttributes, WindowId};

pub use winit::event::{ElementState, MouseButton, WindowEvent};
pub use winit::keyboard::{KeyCode, PhysicalKey};

pub const DEFAULT_WIDTH: u32 = 1600;
pub const DEFAULT_HEIGHT: u32 = 900;

pub struct NativeWindow {
    window: Arc<Window>,
}

impl NativeWindow {
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn set_minimized(&self, minimized: bool) {
        self.window.set_minimized(minimized);
    }

    pub fn inner_size(&self) -> (u32, u32) {
        let size = self.window.inner_size();
        (size.width, size.height)
    }

    pub fn handle(&self) -> &Window {
        &self.window
    }

    pub fn set_title(&self, title: &str) {
        self.window.set_title(title);
    }

    pub fn set_cursor_captured(&self, captured: bool) {
        let _ = self.window.set_cursor_visible(!captured);
        let mode = if captured { CursorGrabMode::Locked } else { CursorGrabMode::None };
        let _ = self.window.set_cursor_grab(mode);
    }

    pub fn center_cursor(&self) {
        let size = self.window.inner_size();
        let _ = self.window.set_cursor_position(winit::dpi::PhysicalPosition::new(size.width / 2, size.height / 2));
    }
}

impl HasWindowHandle for NativeWindow {
    fn window_handle(&self) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        self.window.window_handle()
    }
}

impl HasDisplayHandle for NativeWindow {
    fn display_handle(&self) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        self.window.display_handle()
    }
}

pub trait HostHandler {
    fn on_window(&mut self, window: NativeWindow) -> Result<(), String>;
    fn on_resize(&mut self, width: u32, height: u32) -> Result<(), String>;
    fn on_redraw(&mut self) -> Result<bool, String>;
    fn on_close(&mut self);
    fn on_window_event(&mut self, _event: &WindowEvent) {}
}

pub fn run(handler: &mut dyn HostHandler) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|error| error.to_string())?;
    let mut app = App { handler, failed: None, created: false };
    event_loop.run_app(&mut app).map_err(|error| error.to_string())?;
    match app.failed {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

struct App<'a> {
    handler: &'a mut dyn HostHandler,
    failed: Option<String>,
    created: bool,
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.failed.is_some() || self.created {
            return;
        }
        self.created = true;
        let window = match event_loop.create_window(
            WindowAttributes::default()
                .with_title("JARVIG")
                .with_inner_size(PhysicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT))
                .with_resizable(true),
        ) {
            Ok(window) => NativeWindow { window: Arc::new(window) },
            Err(error) => {
                self.failed = Some(error.to_string());
                event_loop.exit();
                return;
            }
        };
        if let Err(error) = self.handler.on_window(window) {
            self.failed = Some(error);
            event_loop.exit();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.failed.is_some() {
            return;
        }
        self.handler.on_window_event(&event);
        let result = match event {
            WindowEvent::CloseRequested => {
                self.handler.on_close();
                event_loop.exit();
                Ok(())
            }
            WindowEvent::Resized(size) => self.handler.on_resize(size.width, size.height),
            WindowEvent::RedrawRequested => match self.handler.on_redraw() {
                Ok(true) => {
                    self.handler.on_close();
                    event_loop.exit();
                    Ok(())
                }
                other => other.map(|_| ()),
            },
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.failed = Some(error);
            self.handler.on_close();
            event_loop.exit();
        }
    }
}
