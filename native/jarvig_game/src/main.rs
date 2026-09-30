//! JARVIGGame. A project, a runtime world, and one view. Not the editor.

use std::path::PathBuf;
use std::time::Instant;

use jarvig_game::{open_client, project_beside_exe, stage_development, GameSession};
use jarvig_core::PhysicalControl;
use jarvig_platform::{run, ElementState, HostHandler, KeyCode, MouseButton, NativeWindow, PhysicalKey, WindowEvent};
use jarvig_renderer::{FrameOutcome, NormalizedRect, RenderTargetId, RenderViewDesc, RenderViewId, RenderViewSettings, Renderer};
use jarvig_rhi::GraphicsDeviceConfig;

struct Host {
    session: Option<GameSession>,
    window: Option<NativeWindow>,
    renderer: Option<Renderer>,
    target: Option<RenderTargetId>,
    view: Option<RenderViewId>,
    frames: u32,
    limit: Option<u32>,
    last: Instant,
    graphics: GraphicsDeviceConfig,
    scene_meshes: Vec<jarvig_core::MeshId>,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let server = args.iter().any(|arg| arg == "--server");
    let stage = args.iter().any(|arg| arg == "--stage");
    let frames = args.iter().position(|arg| arg == "--frames").and_then(|index| args.get(index + 1).and_then(|value| value.parse::<u32>().ok()));
    let project = match project_arg(&args) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("JARVIG_FAIL {error}");
            eprintln!("usage: JARVIGGame [--server | --stage | --frames N] <project.jarvigproject>");
            std::process::exit(1);
        }
    };
    if stage {
        let host = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("JARVIGGame.exe"));
        match stage_development(&project, &host) {
            Ok(out) => {
                println!("JARVIG_OK staged {}", out.display());
            }
            Err(error) => {
                eprintln!("JARVIG_FAIL {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if server {
        match jarvig_game::open_server(&project) {
            Ok(mut session) => {
                let camera = session.app.active_camera().map(|id| id.to_string()).unwrap_or_else(|| "none".into());
                if session.app.tick(1.0 / 60.0).is_err() {
                    eprintln!("JARVIG_FAIL server tick");
                    std::process::exit(1);
                }
                println!(
                    "JARVIG_OK game server project=\"{}\" level=\"{}\" entities={} camera={camera} extractions={}",
                    session.project_name,
                    session.level_name,
                    session.app.runtime_world().map(|world| world.entity_count()).unwrap_or(0),
                    session.engine.extraction_count()
                );
                let _ = session.app.stop();
            }
            Err(error) => {
                eprintln!("JARVIG_FAIL {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    let session = match open_client(&project) {
        Ok(session) => session,
        Err(error) => {
            eprintln!("JARVIG_FAIL {error}");
            std::process::exit(1);
        }
    };
    let mut host = Host {
        session: Some(session),
        window: None,
        renderer: None,
        target: None,
        view: None,
        frames: 0,
        limit: frames,
        last: Instant::now(),
        graphics: GraphicsDeviceConfig::default(),
        scene_meshes: Vec::new(),
    };
    if let Err(error) = run(&mut host) {
        eprintln!("JARVIG_FAIL {error}");
        std::process::exit(1);
    }
    if let Some(limit) = host.limit {
        if host.frames < limit {
            eprintln!("JARVIG_FAIL presented={} limit={limit}", host.frames);
            std::process::exit(1);
        }
        println!("JARVIG_OK game frames={}", host.frames);
    }
}

fn project_arg(args: &[String]) -> Result<PathBuf, String> {
    let mut skip_next = false;
    for arg in args.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--frames" {
            skip_next = true;
            continue;
        }
        if arg.starts_with("--") {
            continue;
        }
        return Ok(PathBuf::from(arg));
    }
    project_beside_exe(&std::env::current_exe().map_err(|error| error.to_string())?)
}

impl HostHandler for Host {
    fn on_window(&mut self, window: NativeWindow) -> Result<(), String> {
        let session = self.session.as_ref().ok_or("game session missing")?;
        window.set_title(&format!("{} — JARVIGGame", session.project_name));
        let (width, height) = window.inner_size();
        let attached = jarvig_rhi_wgpu::attach_window(window.handle(), width.max(1), height.max(1), &self.graphics).map_err(|error| error.to_string())?;
        println!(
            "JARVIG game graphics {} ({:?}). Startup level {}. This is not the editor.",
            attached.selection.adapter.name, attached.selection.adapter.kind, session.level_name
        );
        let mut renderer = Renderer::new(attached.device, attached.swapchain, attached.color_format, width.max(1), height.max(1));
        let target = renderer.surface_target();
        let camera = session.engine.front_camera();
        let view = renderer
            .create_view(RenderViewDesc {
                label: "JARVIG.Game".into(),
                target,
                camera,
                layout: NormalizedRect::FULL,
                settings: RenderViewSettings::default(),
            })
            .map_err(|error| error.to_string())?;
        self.target = Some(target);
        self.view = Some(view);
        self.renderer = Some(renderer);
        window.set_cursor_captured(session.play.wants_capture());
        window.request_redraw();
        self.window = Some(window);
        Ok(())
    }

    fn on_window_event(&mut self, event: &WindowEvent) {
        let Some(session) = self.session.as_mut() else { return };
        let captured = session.play.wants_capture();
        let device = session.play.device_mut();
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                let Some(control) = physical_key(&event.physical_key) else { return };
                if event.state == ElementState::Pressed {
                    device.held.insert(control);
                } else {
                    device.held.remove(&control);
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                if *state == ElementState::Pressed {
                    device.held.insert(PhysicalControl::MouseLeft);
                } else {
                    device.held.remove(&PhysicalControl::MouseLeft);
                }
            }
            WindowEvent::CursorMoved { position, .. } if captured => {
                if let Some(window) = &self.window {
                    let (width, height) = window.inner_size();
                    let center_x = width as f64 * 0.5;
                    let center_y = height as f64 * 0.5;
                    device.mouse_dx += position.x - center_x;
                    device.mouse_dy += position.y - center_y;
                    window.center_cursor();
                }
            }
            _ => {}
        }
    }

    fn on_resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(width, height).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn on_redraw(&mut self) -> Result<bool, String> {
        let Some(target) = self.target else {
            return Ok(false);
        };
        let now = Instant::now();
        let delta = (now - self.last).as_secs_f64();
        self.last = now;
        let session = self.session.as_mut().ok_or("game session missing")?;
        if session.app.phase() == jarvig_core::ApplicationPhase::Running {
            session.play.tick(session.app.runtime_world_mut().map_err(|error| error.to_string())?, delta)?;
            session.app.tick(delta).map_err(|error| error.to_string())?;
        }
        let captured = session.play.wants_capture();
        if let Some(window) = &self.window {
            window.set_cursor_captured(captured);
        }
        let snapshot = session.app.extract_snapshot().map_err(|error| error.to_string())?;
        let view = self.view.ok_or("game view missing")?;
        if let Some(renderer) = self.renderer.as_mut() {
            let (pose, fov, near) = if let Some(id) = session.app.active_camera() {
                let camera = snapshot.game_cameras().iter().find(|camera| camera.entity == id).ok_or("startup camera was not extracted")?;
                (camera.pose, camera.vertical_fov_radians, camera.near_m)
            } else {
                let front = session.app.runtime_world().ok_or("runtime world missing")?.front_camera();
                let pose = snapshot.camera(front.frame).ok_or("runtime view frame missing")?.pose;
                (pose, front.vertical_fov_radians, front.near_m)
            };
            let frame = session.engine.front_camera().frame;
            renderer
                .update_view(
                    view,
                    jarvig_renderer::RenderViewUpdate {
                        camera: Some(jarvig_core::Camera { frame, vertical_fov_radians: fov, near_m: near }),
                        layout: None,
                        settings: None,
                        pose: Some(pose),
                    },
                )
                .map_err(|error| error.to_string())?;
        }
        let mut fresh = Vec::new();
        for instance in snapshot.instances() {
            if self.scene_meshes.contains(&instance.mesh) {
                continue;
            }
            self.scene_meshes.push(instance.mesh);
            fresh.push((instance.mesh, session.engine.meshlet_records(instance.entity)));
        }
        let meshes = session.app.runtime_world().ok_or("runtime world missing")?.meshes() as *const _;
        let materials = session.engine.materials() as *const _;
        let textures = session.engine.textures() as *const _;
        let renderer = self.renderer.as_mut().ok_or("renderer missing")?;
        for (mesh, records) in &fresh {
            renderer.remember_meshlets(*mesh, records);
        }
        let outcome = unsafe { renderer.render_target(target, &snapshot, &*meshes, &*materials, &*textures).map_err(|error| error.to_string())? };
        if matches!(outcome, FrameOutcome::Presented { .. }) {
            self.frames = self.frames.saturating_add(1);
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        Ok(self.limit.is_some_and(|limit| self.frames >= limit))
    }

    fn on_close(&mut self) {
        if let Some(session) = self.session.as_mut() {
            let _ = session.app.stop();
        }
        if let Some(renderer) = self.renderer.as_mut() {
            let _ = renderer.flush();
        }
    }
}

fn physical_key(key: &PhysicalKey) -> Option<PhysicalControl> {
    let PhysicalKey::Code(code) = key else { return None };
    Some(match code {
        KeyCode::KeyW => PhysicalControl::KeyW,
        KeyCode::KeyA => PhysicalControl::KeyA,
        KeyCode::KeyS => PhysicalControl::KeyS,
        KeyCode::KeyD => PhysicalControl::KeyD,
        KeyCode::KeyQ => PhysicalControl::KeyQ,
        KeyCode::KeyE => PhysicalControl::KeyE,
        KeyCode::Space => PhysicalControl::Space,
        KeyCode::ShiftLeft | KeyCode::ShiftRight => PhysicalControl::Shift,
        KeyCode::ControlLeft | KeyCode::ControlRight => PhysicalControl::Control,
        KeyCode::Escape => PhysicalControl::Escape,
        _ => return None,
    })
}
