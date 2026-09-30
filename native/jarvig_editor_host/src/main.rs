//! Transitional native host. Not the TypeScript dock and not JARVIGEditor.exe.
//!
//! The host owns the process. The engine owns frame order. The renderer owns
//! the frame. This file does not name the shader, the pipeline, wgpu, or the window crate.

use std::time::Instant;

use jarvig_engine::{EngineSession, FrameError};
use jarvig_platform::{run, HostHandler, NativeWindow, DEFAULT_HEIGHT, DEFAULT_WIDTH};
use jarvig_rhi::{AdapterFingerprint, AdapterKind, GraphicsAdapterDesc, GraphicsApi, GraphicsDeviceConfig, GraphicsPreference, GraphicsSelection};
use jarvig_renderer::{
    default_clear, FrameOutcome, NormalizedRect, RenderError, RenderTargetId, RenderViewDesc, RenderViewId, RenderViewSettings,
    Renderer,
};

struct Host {
    engine: EngineSession,
    window: Option<NativeWindow>,
    renderer: Option<Renderer>,
    target: Option<RenderTargetId>,
    front: Option<RenderViewId>,
    side: Option<RenderViewId>,
    frames: u32,
    limit: Option<u32>,
    self_test: bool,
    list_gpus: bool,
    graphics: GraphicsDeviceConfig,
    tested_zero_size: bool,
    last: Instant,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let self_test = args.iter().any(|arg| arg == "--self-test");
    let limit = args.iter().position(|arg| arg == "--frames").and_then(|index| {
        args.get(index + 1).and_then(|value| value.parse::<u32>().ok())
    });
    let (graphics, list_gpus) = match graphics_config(&args) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("JARVIG_FAIL {error}");
            std::process::exit(1);
        }
    };
    let mut host = Host {
        engine: EngineSession::editor().expect("engine"),
        window: None,
        renderer: None,
        target: None,
        front: None,
        side: None,
        frames: 0,
        limit: if self_test { Some(4) } else { limit },
        self_test,
        list_gpus,
        graphics,
        tested_zero_size: false,
        last: Instant::now(),
    };
    if let Err(error) = run(&mut host) {
        eprintln!("JARVIG_FAIL {error}");
        std::process::exit(1);
    }
    if host.self_test && (host.frames < 4 || !host.tested_zero_size) {
        eprintln!("JARVIG_FAIL presented={} zero_size={}", host.frames, host.tested_zero_size);
        std::process::exit(1);
    }
    println!("JARVIG_OK native-present frames={}", host.frames);
}

impl HostHandler for Host {
    fn on_window(&mut self, window: NativeWindow) -> Result<(), String> {
        let (width, height) = window.inner_size();
        if self.list_gpus {
            let survey = jarvig_rhi_wgpu::describe_adapters(window.handle(), &self.graphics).map_err(|error| error.to_string())?;
            print_adapter_list(&survey.adapters, &survey.selection);
            if let Err(error) = &survey.selection {
                println!("JARVIG_FAIL {error}");
                std::process::exit(1);
            }
            std::process::exit(0);
        }
        let attached = jarvig_rhi_wgpu::attach_window(window.handle(), width, height, &self.graphics).map_err(|error| error.to_string())?;
        print_graphics(&attached.selection, attached.color_format, &attached.adapters);
        let performance = matches!(self.graphics.preference, GraphicsPreference::Auto | GraphicsPreference::HighPerformance);
        let discrete_available = attached.adapters.iter().any(|adapter| {
            adapter.kind == AdapterKind::DiscreteGpu && adapter.surface_compatible && adapter.meets_requirements && !adapter.software
        });
        if self.self_test && performance && discrete_available && attached.selection.adapter.kind != AdapterKind::DiscreteGpu {
            return Err(format!(
                "high-performance policy ignored a compatible discrete adapter and selected {}",
                attached.selection.adapter.name
            ));
        }
        let mut renderer = Renderer::new(
            attached.device,
            attached.swapchain,
            attached.color_format,
            width.max(1),
            height.max(1),
        );
        let target = renderer.surface_target();
        let front = renderer
            .create_view(RenderViewDesc {
                label: "JARVIG.Perspective".into(),
                target,
                camera: self.engine.front_camera(),
                layout: NormalizedRect::LEFT,
                settings: RenderViewSettings::default(),
            })
            .map_err(|error| error.to_string())?;
        let side = renderer
            .create_view(RenderViewDesc {
                label: "JARVIG.Alternate".into(),
                target,
                camera: self.engine.side_camera(),
                layout: NormalizedRect::RIGHT,
                settings: RenderViewSettings::default(),
            })
            .map_err(|error| error.to_string())?;
        println!(
            "JARVIG views={} {} {} surface={} clear=({}, {}, {}, {})",
            renderer.view_count(),
            renderer.view_label(front).map_err(|error| error.to_string())?,
            renderer.view_label(side).map_err(|error| error.to_string())?,
            Renderer::surface_label(),
            default_clear().r,
            default_clear().g,
            default_clear().b,
            default_clear().a
        );
        let _ = (DEFAULT_WIDTH, DEFAULT_HEIGHT);
        self.target = Some(target);
        self.front = Some(front);
        self.side = Some(side);
        self.renderer = Some(renderer);
        self.last = Instant::now();
        window.request_redraw();
        self.window = Some(window);
        Ok(())
    }

    fn on_resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(width, height).map_err(|error| error.to_string())?;
        }
        println!("JARVIG resize {width}x{height}");
        Ok(())
    }

    fn on_redraw(&mut self) -> Result<bool, String> {
        let Some(target) = self.target else {
            return Ok(false);
        };
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f64();
        self.last = now;
        let renderer = self.renderer.as_mut().ok_or("renderer missing")?;
        let mut outcome = FrameOutcome::TimedOut;
        let tick = self
            .engine
            .run_frame(dt, |snapshot, meshes, materials, textures| {
                outcome = renderer.render_target(target, snapshot, meshes, materials, textures)?;
                Ok(())
            })
            .map_err(describe)?;
        if !tick.render_executed {
            return Err("editor profile skipped the renderer".into());
        }
        if matches!(outcome, FrameOutcome::Presented { .. }) {
            self.frames = self.frames.saturating_add(1);
            println!("JARVIG present frame={}", self.frames);
        } else {
            println!("JARVIG present skipped {outcome:?}");
        }
        if self.self_test && !self.tested_zero_size && self.frames >= 1 {
            self.zero_size_then_restore()?;
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        let stop = self.limit.is_some_and(|limit| self.frames >= limit);
        if stop && self.self_test {
            let renderer = self.renderer.as_mut().ok_or("renderer missing")?;
            renderer.flush().map_err(|error| error.to_string())?;
            let pipelines = renderer.pipeline_count();
            let buffers = renderer.buffer_count();
            let indexed = renderer.indexed_draw_count();
            let draws = renderer.draw_count();
            let meshes = self.engine.mesh_count();
            let front = self.front.ok_or("front view missing")?;
            let side = self.side.ok_or("side view missing")?;
            let front_depth = renderer.depth_size(front).map_err(|error| error.to_string())?;
            let side_depth = renderer.depth_size(side).map_err(|error| error.to_string())?;
            let front_viewport = renderer.viewport(front).map_err(|error| error.to_string())?;
            let side_viewport = renderer.viewport(side).map_err(|error| error.to_string())?;
            let stats = renderer.resource_stats();
            let half = DEFAULT_WIDTH / 2;
            if pipelines != 7
                || buffers != 31
                || indexed != u64::from(self.frames) * 8 + 42
                || draws != u64::from(self.frames) * 10 + 108
                || front_depth != Some((DEFAULT_WIDTH, DEFAULT_HEIGHT))
                || side_depth != Some((DEFAULT_WIDTH, DEFAULT_HEIGHT))
                || meshes != 2
                || renderer.mesh_upload_count() != 2
                || renderer.submitted_last() != 2
                || !self.engine.extracted_this_frame()
                || self.engine.last_visible_count() != 2
                || self.engine.last_instance_count() != 2
                || self.engine.material_master_count() != 1
                || self.engine.material_instance_count() != 2
                || self.engine.material_compile_count() != 1
                || renderer.material_pipeline_count() != 1
                || renderer.material_parameter_upload_count() != 2
                || renderer.texture_upload_count() != 5
                || renderer.gpu_texture_count() != 5
                || self.engine.pbr_master_count() != 1
                || self.engine.pbr_instance_count() != 2
                || self.engine.pbr_compile_count() != 1
                || renderer.gpu_sampler_count() != 1
                || renderer.material_texture_binding_updates() != 0
                || self.engine.texture_count() != 9
                || self.engine.sampler_count() != 1
                || renderer.unbound_material_skip_count() != 0
                || renderer.view_count() != 2
                || renderer.target_count() != 1
                || renderer.acquires_last_frame() != 1
                || renderer.presents_last_frame() != 1
                || stats.alive_buffers != 24
                || stats.alive_pipelines != 4
                || stats.alive_textures != 19
                || stats.alive_texture_views != 25
                || stats.alive_samplers != 4
                || stats.alive_shaders != 4
                || stats.alive_bind_groups != 12
                || stats.alive_layouts != 5
                || renderer.gpu_light_packet_count() != 2
                || renderer.light_buffer_upload_count() != 4
                || renderer.environment_packet_upload_count() != 1
                || self.engine.world_light_count() != 3
                || self.engine.last_light_count() != 3
                || self.engine.directional_light_count() != 1
                || self.engine.point_light_count() != 1
                || self.engine.spot_light_count() != 1
                || stats.retired != 0
                || front_viewport != Some(jarvig_renderer::PixelRect { x: 0, y: 0, width: half, height: DEFAULT_HEIGHT })
                || side_viewport != Some(jarvig_renderer::PixelRect { x: half, y: 0, width: DEFAULT_WIDTH - half, height: DEFAULT_HEIGHT })
            {
                return Err(format!(
                    "pipeline_count={pipelines} material_pipelines={} compiles={} parameter_uploads={} texture_uploads={} buffer_count={buffers} indexed_draw_count={indexed} draw_count={draws} frames={} alive_buffers={} alive_groups={} alive_layouts={} retired={} textures={} views={} uploads={} acquires={}",
                    renderer.material_pipeline_count(),
                    self.engine.material_compile_count(),
                    renderer.material_parameter_upload_count(),
                    renderer.texture_upload_count(),
                    self.frames,
                    stats.alive_buffers,
                    stats.alive_bind_groups,
                    stats.alive_layouts,
                    stats.retired,
                    stats.alive_textures,
                    stats.alive_texture_views,
                    renderer.mesh_upload_count(),
                    renderer.acquires_last_frame()
                ));
            }
            println!(
                "JARVIG pipeline_count={pipelines} material_master_count={} material_instance_count={} material_compile_count={} pbr_master_count={} pbr_instance_count={} pbr_compile_count={} material_pipeline_count={} material_parameter_upload_count={} texture_count={} texture_upload_count={} sampler_count={} gpu_sampler_count={} world_light_count={} snapshot_light_count={} directional_light_count={} point_light_count={} spot_light_count={} gpu_light_packet_count={} light_buffer_upload_count={} buffer_count={buffers} mesh_count={meshes} scene_instances={} view_count=2 target_count=1 extractions_this_frame=1 indexed_draw_count={indexed} draw_count={draws} depth={front_depth:?} alive_buffers={} retired={} acquires={} presents={}",
                self.engine.material_master_count(),
                self.engine.material_instance_count(),
                self.engine.material_compile_count(),
                self.engine.pbr_master_count(),
                self.engine.pbr_instance_count(),
                self.engine.pbr_compile_count(),
                renderer.material_pipeline_count(),
                renderer.material_parameter_upload_count(),
                self.engine.texture_count(),
                renderer.texture_upload_count(),
                self.engine.sampler_count(),
                renderer.gpu_sampler_count(),
                self.engine.world_light_count(),
                self.engine.last_light_count(),
                self.engine.directional_light_count(),
                self.engine.point_light_count(),
                self.engine.spot_light_count(),
                renderer.gpu_light_packet_count(),
                renderer.light_buffer_upload_count(),
                self.engine.last_visible_count(),
                stats.alive_buffers,
                stats.retired,
                renderer.acquires_last_frame(),
                renderer.presents_last_frame()
            );
        }
        Ok(stop)
    }

    fn on_close(&mut self) {
        self.renderer = None;
        self.window = None;
        println!("JARVIG shutdown");
    }
}

impl Host {
    fn zero_size_then_restore(&mut self) -> Result<(), String> {
        let renderer = self.renderer.as_mut().ok_or("renderer missing")?;
        let before_depth = renderer.depth_target_count();
        renderer.resize(0, 0).map_err(|error| error.to_string())?;
        let front = self.front.ok_or("front view missing")?;
        let side = self.side.ok_or("side view missing")?;
        if renderer.depth_target_count() != before_depth
            || renderer.depth_size(front).map_err(|error| error.to_string())?.is_some()
            || renderer.depth_size(side).map_err(|error| error.to_string())?.is_some()
        {
            return Err("zero size created a depth target".into());
        }
        let stats = renderer.resource_stats();
        if stats.alive_textures
            != renderer.gpu_texture_count() as u32
                + renderer.reflection_probe_texture_count()
                + renderer.shadow_texture_count()
                + renderer.indirect_diffuse_texture_count()
            || stats.retired != 0
        {
            return Err(format!(
                "zero size should keep sampled textures and the probe cube alive={} retired={}",
                stats.alive_textures, stats.retired
            ));
        }
        let target = self.target.ok_or("target missing")?;
        let mut skipped = FrameOutcome::TimedOut;
        self.engine
            .run_frame(0.0, |snapshot, meshes, materials, textures| {
                skipped = renderer.render_target(target, snapshot, meshes, materials, textures)?;
                Ok(())
            })
            .map_err(describe)?;
        if skipped != FrameOutcome::Minimized {
            return Err(format!("zero size should skip, got {skipped:?}"));
        }
        println!("JARVIG minimize-path skipped");
        if let Some(window) = &self.window {
            window.set_minimized(true);
            window.set_minimized(false);
        }
        renderer
            .resize(DEFAULT_WIDTH, DEFAULT_HEIGHT)
            .map_err(|error| error.to_string())?;
        if renderer.depth_target_count() != before_depth + 2
            || renderer.depth_size(front).map_err(|error| error.to_string())? != Some((DEFAULT_WIDTH, DEFAULT_HEIGHT))
            || renderer.depth_size(side).map_err(|error| error.to_string())? != Some((DEFAULT_WIDTH, DEFAULT_HEIGHT))
            || renderer.view_count() != 2
        {
            return Err(format!(
                "depth was not recreated at {}x{}, count {}",
                DEFAULT_WIDTH,
                DEFAULT_HEIGHT,
                renderer.depth_target_count()
            ));
        }
        self.tested_zero_size = true;
        println!("JARVIG restore-path configured");
        Ok(())
    }
}

fn graphics_config(args: &[String]) -> Result<(GraphicsDeviceConfig, bool), String> {
    let mut config = GraphicsDeviceConfig::default();
    let mut list = false;
    let mut index = 1usize;
    while index < args.len() {
        match args[index].as_str() {
            "--list-gpus" => list = true,
            "--no-software" => config.allow_software_fallback = false,
            "--frames" => index += 1,
            "--gpu" => {
                let value = args.get(index + 1).ok_or("--gpu needs auto, high-performance, low-power, or vendor:device:api")?;
                config.preference = parse_gpu(value)?;
                index += 1;
            }
            other => {
                if let Some(value) = other.strip_prefix("--gpu=") {
                    config.preference = parse_gpu(value)?;
                }
            }
        }
        index += 1;
    }
    Ok((config, list))
}

fn parse_gpu(value: &str) -> Result<GraphicsPreference, String> {
    match value {
        "auto" => Ok(GraphicsPreference::Auto),
        "high-performance" | "high" => Ok(GraphicsPreference::HighPerformance),
        "low-power" | "low" => Ok(GraphicsPreference::LowPower),
        other => {
            let parts: Vec<_> = other.split(':').collect();
            if parts.len() != 3 {
                return Err(format!("unknown --gpu value '{other}'. Use auto, high-performance, low-power, or vendor:device:api"));
            }
            Ok(GraphicsPreference::Specific(AdapterFingerprint {
                vendor_id: parse_id(parts[0])?,
                device_id: parse_id(parts[1])?,
                api: GraphicsApi::parse(parts[2]).ok_or_else(|| format!("unknown graphics api '{}'", parts[2]))?,
                kind: None,
                name: None,
            }))
        }
    }
}

fn parse_id(text: &str) -> Result<u32, String> {
    let stripped = text.trim().trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(stripped, 16).map_err(|_| format!("adapter id '{text}' is not hexadecimal"))
}

fn print_graphics(selection: &GraphicsSelection, color: impl std::fmt::Debug, adapters: &[GraphicsAdapterDesc]) {
    let selected = &selection.adapter;
    println!("JARVIG Graphics Adapter");
    println!("Policy: {}", selection.preference.label());
    println!("Selected: {}", selected.name);
    println!("Class: {:?}", selected.kind);
    println!("Backend: {}", selected.api.label());
    println!("Vendor ID: {:04x}", selected.vendor_id);
    println!("Device ID: {:04x}", selected.device_id);
    println!("Driver: {} {}", selected.driver, selected.driver_info);
    println!(
        "Limits: texture={} storage_buffers={} color={color:?}",
        selected.max_texture_dimension_2d, selected.max_storage_buffers_per_shader_stage
    );
    println!("Reason: {}", selection.reason);
    print_adapter_list(adapters, &Ok(selection.clone()));
}

fn print_adapter_list(adapters: &[GraphicsAdapterDesc], selection: &Result<GraphicsSelection, String>) {
    if let Ok(selection) = selection {
        println!("JARVIG gpu policy={}", selection.preference.label());
        println!("JARVIG gpu reason={}", selection.reason);
        println!("JARVIG gpu fallback={}", selection.fell_back);
    }
    for adapter in adapters {
        let chosen = selection.as_ref().ok().is_some_and(|pick| pick.adapter.runtime_id == adapter.runtime_id);
        println!(
            "JARVIG gpu {} name={} class={:?} api={} vendor={:04x} device={:04x} surface={} requirements={} software={} note={}",
            if chosen { "selected" } else { "candidate" },
            adapter.name,
            adapter.kind,
            adapter.api.label(),
            adapter.vendor_id,
            adapter.device_id,
            adapter.surface_compatible,
            adapter.meets_requirements,
            adapter.software,
            adapter.rejection.as_deref().unwrap_or("-")
        );
    }
}

fn describe(error: FrameError<RenderError>) -> String {
    match error {
        FrameError::Runtime(error) => format!("runtime {error:?}"),
        FrameError::Render(error) => error.to_string(),
        FrameError::Scene(error) => error.to_string(),
    }
}
