//! Project and level files. The editor does not own the world, and the files do not own the GPU.
//!
//! Editor camera, dock layout, exposure, and lighting debug stay out of `.jarviglevel`.
//! A viewport pose and the editor workspace may be written under `Saved/Editor` and are not gameplay truth.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use jarvig_core::{
    autosave_path, create_project_directories, empty_world_level, load_level_file, load_project_file, parse_character, save_level_atomic,
    save_project_atomic, EntityId, LevelDocument, ProjectDocument, RegistryAsset, Vec3,
};
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OPENFILENAMEW, OFN_FILEMUSTEXIST, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDCANCEL, IDNO, IDYES, MB_YESNOCANCEL};

use super::{wide, Editor};

const AUTOSAVE_SECONDS: f64 = 60.0;

fn material_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/materials")
}

impl Editor {
    fn yield_result<T: Send + 'static>(&mut self, phase: &str, label: &str, fraction: Option<f32>, job: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
        self.report(phase, label, fraction);
        self.pump_loading();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(job());
        });
        loop {
            match receiver.try_recv() {
                Ok(value) => return value,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.pump_loading();
                    std::thread::sleep(std::time::Duration::from_millis(16));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => return Err(format!("{label} stopped before it finished")),
            }
        }
    }

    fn install_staged_materials(&mut self, document: &LevelDocument) -> Result<(), String> {
        use jarvig_core::{
            classify_material_file, negate_normal_green, pack_orm_rgba8, select_normal, ColorSpace, MaterialMapRole, MaterialScheme, Texture,
        };
        let mut names = Vec::new();
        for entity in &document.entities {
            for component in &entity.components {
                if let jarvig_core::ComponentRecord::MeshRenderer { material, .. } = component {
                    if material.scheme == MaterialScheme::Staged && !names.iter().any(|stored: &String| stored == &material.name) {
                        names.push(material.name.clone());
                    }
                }
            }
        }
        let staged = names.len();
        for name in names {
            let folder = material_root().join(format!("{name}_4K-PNG"));
            let entries = fs::read_dir(&folder).map_err(|_| format!("staged material {name} was not found in {}", folder.display()))?;
            let mut files = Vec::new();
            for entry in entries {
                let entry = entry.map_err(|error| error.to_string())?;
                if let Some(file) = entry.file_name().to_str() {
                    files.push(file.to_string());
                }
            }
            let classified: Vec<_> = files.iter().filter_map(|file| classify_material_file(&name, file).map(|role| (file.clone(), role))).collect();
            let color_file = classified.iter().find(|(_, role)| *role == MaterialMapRole::BaseColor).map(|(file, _)| file.clone()).ok_or_else(|| format!("{name} has no Color map"))?;
            let (source_convention, flip_green) = select_normal(&classified.iter().map(|(_, role)| *role).collect::<Vec<_>>()).ok_or_else(|| format!("{name} has no normal map"))?;
            let normal_file = classified
                .iter()
                .find(|(_, role)| *role == MaterialMapRole::Normal(source_convention))
                .map(|(file, _)| file.clone())
                .ok_or_else(|| format!("{name} normal file is missing"))?;
            let rough_file = classified.iter().find(|(_, role)| *role == MaterialMapRole::Roughness).map(|(file, _)| file.clone());
            let ao_file = classified.iter().find(|(_, role)| *role == MaterialMapRole::AmbientOcclusion).map(|(file, _)| file.clone());
            let metal_file = classified.iter().find(|(_, role)| *role == MaterialMapRole::Metallic).map(|(file, _)| file.clone());
            let height = classified.iter().any(|(_, role)| *role == MaterialMapRole::Height);
            let mut maps = vec![
                (format!("{name} color"), folder.join(&color_file), jarvig_core::MipContent::Srgb, false),
                (format!("{name} normal"), folder.join(&normal_file), jarvig_core::MipContent::Normal, flip_green),
            ];
            if let Some(file) = &rough_file {
                maps.push((format!("{name} roughness"), folder.join(file), jarvig_core::MipContent::Linear, false));
            }
            if let Some(file) = &ao_file {
                maps.push((format!("{name} ambient occlusion"), folder.join(file), jarvig_core::MipContent::Linear, false));
            }
            if let Some(file) = &metal_file {
                maps.push((format!("{name} metalness"), folder.join(file), jarvig_core::MipContent::Linear, false));
            }
            let total = maps.len() as u32 + 3;
            let mut decoded = Vec::new();
            for (index, (label, path, content, flip)) in maps.into_iter().enumerate() {
                let fraction = Some(index as f32 / total as f32);
                decoded.push(self.yield_result("Loading materials", &label, fraction, move || {
                    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
                    let (width, height, pixels) = crate::png_decode::decode_png_rgba8(&bytes)?;
                    let (width, height, mut pixels) = jarvig_core::downsample_long_side(width, height, pixels, 2048, content);
                    if flip {
                        negate_normal_green(&mut pixels);
                    }
                    Ok((width, height, pixels))
                })?);
            }
            let (width, height_px, color) = decoded.remove(0);
            let (_, _, normal) = decoded.remove(0);
            if normal.len() != color.len() {
                return Err(format!("{name} normal size does not match the color map"));
            }
            let mut rest = decoded.into_iter();
            let rough = if rough_file.is_some() { rest.next().map(|(_, _, pixels)| pixels) } else { None };
            let ao = if ao_file.is_some() { rest.next().map(|(_, _, pixels)| pixels) } else { None };
            let metal = if metal_file.is_some() { rest.next().map(|(_, _, pixels)| pixels) } else { None };
            let orm = pack_orm_rgba8(width, height_px, ao.as_deref(), rough.as_deref(), metal.as_deref()).map_err(|error| error.to_string())?;
            let color_label = format!("{name} color mips");
            let normal_label = format!("{name} normal mips");
            let orm_label = format!("{name} packed maps");
            let color_tex = self.yield_result("Loading materials", &color_label, Some((total - 3) as f32 / total as f32), move || {
                Texture::with_mips(width, height_px, ColorSpace::Srgb, color, jarvig_core::MipContent::Srgb).map_err(|error| format!("{error:?}"))
            })?;
            let normal_tex = self.yield_result("Loading materials", &normal_label, Some((total - 2) as f32 / total as f32), move || {
                Texture::with_mips(width, height_px, ColorSpace::Linear, normal, jarvig_core::MipContent::Normal).map_err(|error| format!("{error:?}"))
            })?;
            let orm_tex = self.yield_result("Loading materials", &orm_label, Some((total - 1) as f32 / total as f32), move || {
                Texture::with_mips(width, height_px, ColorSpace::Linear, orm, jarvig_core::MipContent::Linear).map_err(|error| format!("{error:?}"))
            })?;
            let mip_count = color_tex.mip_count();
            self.engine.register_staged_material(&name, color_tex, orm_tex, normal_tex);
            let flip = if flip_green { "green negated once at ingest" } else { "no green flip" };
            let line = format!(
                "Material {name}: base color sRGB, normal {} ({flip}), roughness/AO/metallic linear, displacement {} , ingested {width}x{height_px}, mips {}, anisotropy 8. The level stores the set name, not the pixels.",
                source_convention.label(),
                if height { "recognized, not sampled" } else { "absent" },
                mip_count
            );
            eprintln!("JARVIG {line}");
            self.append(&line);
        }
        eprintln!(
            "JARVIG Material path: {staged} staged sets, {} shader compiles. The shader does not name a set or a filename.",
            self.engine.material_compile_count()
        );
        Ok(())
    }

    pub(super) fn boot_project(&mut self) -> Result<(), String> {
        if self.self_test {
            return Ok(());
        }
        if let Some(path) = self.startup_project.clone() {
            self.open_project_at(&path)?;
            self.append(&format!("Opened project {} from --project.", path.display()));
            return Ok(());
        }
        if let Some(path) = find_up("samples/lighting-lab/LightingLab.jarvigproject") {
            match self.open_project_at(&path) {
                Ok(()) => {
                    self.append(&format!("Opened project {} and its startup level from disk.", path.display()));
                    return Ok(());
                }
                Err(error) => {
                    eprintln!("JARVIG Lighting Lab project was found but did not load: {error}");
                    self.append(&format!("Lighting Lab project was found but did not load: {error}"));
                }
            }
        }
        self.engine.world_mut().set_reflection_probe_resolution(64).map_err(|error| error.to_string())?;
        self.engine.install_lighting_lab()?;
        self.saved_revision = self.engine.world().revision();
        self.append("Lighting Lab project file was not found. The lab was built in memory. Use File > Save As to write a level.");
        Ok(())
    }

    pub(super) fn level_dirty(&self) -> bool {
        if self.self_test || self.level_name.is_empty() {
            return false;
        }
        self.unsaved_policy || self.engine.world().revision() != self.saved_revision
    }

    pub(super) fn refresh_title(&self) {
        if self.frame.is_null() {
            return;
        }
        let star = if self.level_dirty() { "*" } else { "" };
        let title = if self.character_workspace && !self.level_name.is_empty() {
            format!("Character: {}{star} - JARVIGEditor", self.level_name)
        } else if self.land_mode && !self.level_name.is_empty() {
            format!("Land: {}{star} - JARVIGEditor", self.level_name)
        } else if self.level_name.is_empty() {
            "JARVIGEditor".to_string()
        } else if self.project_name.is_empty() {
            format!("{}{star} - JARVIGEditor", self.level_name)
        } else {
            format!("{}{star} - {} - JARVIGEditor", self.level_name, self.project_name)
        };
        if super::window_text(self.frame) == title {
            return;
        }
        let text = wide(&title);
        unsafe { windows_sys::Win32::UI::WindowsAndMessaging::SetWindowTextW(self.frame, text.as_ptr()); }
    }

    pub(super) fn maybe_autosave(&mut self) {
        if self.self_test || !self.level_dirty() {
            return;
        }
        let Some(project_file) = self.project_file.clone() else { return };
        let Some(level_file) = self.level_file.clone() else { return };
        if self.last_autosave.elapsed().as_secs_f64() < AUTOSAVE_SECONDS {
            return;
        }
        let Ok(project) = load_project_file(&project_file) else { return };
        let Some(name) = level_file.file_name().and_then(|name| name.to_str()) else { return };
        let Ok(path) = autosave_path(&project_file, &project, name) else { return };
        if path == level_file {
            return;
        }
        let Some(uuid) = self.level_uuid else { return };
        let Ok(document) = self.engine.export_level(uuid, &self.level_name) else { return };
        let backup = project_file.parent().unwrap_or(Path::new(".")).join("Saved/Backup");
        if save_level_atomic(&path, &backup, &document).is_ok() {
            self.last_autosave = Instant::now();
            self.append(&format!("Autosaved to {}. The authored level was not overwritten.", path.display()));
        }
    }

    pub(super) fn file_new_project(&mut self) {
        self.file_new_from_template("empty");
    }

    /// Empty World, Terrain World, Third Person, or FPS. The level is World Settings only.
    pub(super) fn file_new_from_template(&mut self, template: &str) {
        self.stop_play();
        if !self.confirm_save_or_discard() {
            return;
        }
        let Some(path) = save_dialog(self.frame, "New JARVIG Project", "JARVIG Project\0*.jarvigproject\0", "jarvigproject") else { return };
        let path = ensure_extension(&path, "jarvigproject");
        let name = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("Project").to_string();
        let mut project = ProjectDocument::lighting_lab();
        project.display_name = name.clone();
        project.project_uuid = EntityId::new();
        project.startup_level = format!("Content/Levels/{name}.jarviglevel");
        if let Err(error) = create_project_directories(&path, &project) {
            self.append(&format!("Project folders were not created: {error}"));
            return;
        }
        let backup = path.parent().unwrap_or(Path::new(".")).join("Saved/Backup");
        if let Err(error) = save_project_atomic(&path, &backup, &project) {
            self.append(&format!("Project file was not written: {error}"));
            return;
        }
        let mut level = empty_world_level();
        level.name = name;
        let level_path = match project.startup_level_path(&path) {
            Ok(path) => path,
            Err(error) => {
                self.append(&error.to_string());
                return;
            }
        };
        if let Err(error) = save_level_atomic(&level_path, &backup, &level) {
            self.append(&format!("Startup level was not written: {error}"));
            return;
        }
        let settings = path.parent().unwrap_or(Path::new(".")).join(&project.settings);
        let _ = fs::write(settings, "JARVIG project settings placeholder. This is not a level and not a GPU resource.\n");
        write_template_file(&path, &project, template);
        if let Err(error) = self.open_project_at(&path) {
            self.append(&format!("Project was written but did not open: {error}"));
        }
    }

    pub(super) fn file_open_project(&mut self) {
        self.stop_play();
        if !self.confirm_save_or_discard() {
            return;
        }
        let Some(path) = open_dialog(self.frame, "Open JARVIG Project", "JARVIG Project\0*.jarvigproject\0") else { return };
        if let Err(error) = self.open_project_at(&path) {
            self.fail_progress("Opening project", &error);
            self.append(&format!("Project did not open: {error}"));
        }
    }

    pub(super) fn file_close_project(&mut self) {
        self.stop_play();
        if !self.confirm_save_or_discard() {
            return;
        }
        self.project_file = None;
        self.project_name.clear();
        self.level_file = None;
        self.level_name.clear();
        self.level_uuid = None;
        self.unsaved_policy = false;
        self.saved_revision = self.engine.world().revision();
        self.append("Project closed. The viewport still shows the last world. Open a project or level to attach a file.");
        self.refresh_title();
    }

    pub(super) fn file_new_level(&mut self) {
        self.stop_play();
        if !self.confirm_save_or_discard() {
            return;
        }
        let Some(path) = save_dialog(self.frame, "New JARVIG Level", "JARVIG Level\0*.jarviglevel\0", "jarviglevel") else { return };
        let path = ensure_extension(&path, "jarviglevel");
        let name = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("Level").to_string();
        let mut document = empty_world_level();
        document.name = name;
        if let Err(error) = self.store_level(&path, &document, true) {
            self.append(&format!("New level was not written: {error}"));
            return;
        }
        if let Err(error) = self.adopt_level(document, Some(path)) {
            self.append(&format!("New level was written but did not become the world: {error}"));
        } else {
            self.restore_workspace();
        }
    }

    pub(super) fn file_open_level(&mut self) {
        self.stop_play();
        if !self.confirm_save_or_discard() {
            return;
        }
        let Some(path) = open_dialog(self.frame, "Open JARVIG Level", "JARVIG Level\0*.jarviglevel\0") else { return };
        if let Err(error) = self.load_level_from(&path, &path.display().to_string()) {
            self.fail_progress("Loading level file", &error);
            self.append(&format!("Level was not opened. The current world is unchanged. {error}"));
        }
    }

    pub(super) fn file_import_mesh(&mut self) {
        if self.session_active() {
            self.append("Stop play before importing a mesh. The import would not be saved into the runtime world.");
            return;
        }
        let Some(project_file) = self.project_file.clone() else {
            self.append("Open a project before importing a mesh.");
            return;
        };
        let Some(path) = open_dialog(self.frame, "Import glTF Mesh", "glTF mesh\0*.glb;*.gltf\0") else { return };
        let project = match load_project_file(&project_file) {
            Ok(project) => project,
            Err(error) => {
                self.append(&format!("Import did not start. {error}"));
                return;
            }
        };
        self.append(&format!(
            "Importing {}. The external file stays where it is. A copy is written under Content/Meshes, then the JARVIG mesh is built.",
            path.display()
        ));
        self.pump_loading();
        match self.engine.import_and_place_mesh(&project_file, &project.content_directory, &project.intermediate_directory, &path) {
            Ok(entity) => {
                self.game.set_mesh_assets(self.engine.mesh_asset_library().clone());
                if let Ok(item) = crate::selection::SelectionItem::entity(entity) {
                    let _ = self.selection.replace(item);
                }
                self.sync_outliner();
                self.refresh_title();
                let asset_id = match self.engine.world().authored_mesh(entity) {
                    Some((_, _, _, _, _, _, jarvig_core::MeshAssetRef::Asset { id, .. }, _)) => Some(id),
                    _ => None,
                };
                let note = asset_id.and_then(|id| self.engine.mesh_asset_library().record(id)).map(|record| {
                    format!(
                        "Imported {} as asset {}. {} triangles, {} submeshes, bounds [{:.3}, {:.3}, {:.3}] to [{:.3}, {:.3}, {:.3}]. Project copy: {}. External source: {}. The actor uses the mesh's own material, not a lab tile set. Meshes over 250000 triangles receive shadows and are left out of shadow-map redraws. The level stores the asset id. Save the level to keep the actor.",
                        record.name,
                        record.id,
                        record.triangle_count,
                        record.submesh_count,
                        record.bounds_min[0],
                        record.bounds_min[1],
                        record.bounds_min[2],
                        record.bounds_max[0],
                        record.bounds_max[1],
                        record.bounds_max[2],
                        record.source_relative,
                        if record.external_source.is_empty() { "not recorded".into() } else { record.external_source.clone() }
                    )
                });
                self.append(&note.unwrap_or_else(|| "Mesh imported as one asset and placed as one actor. Save the level to keep the actor.".into()));
            }
            Err(error) => self.append(&format!("Mesh import failed. No asset was added. {error}")),
        }
    }

    pub(super) fn file_save(&mut self) {
        let Some(path) = self.level_file.clone() else {
            self.file_save_as();
            return;
        };
        if let Err(error) = self.save_level_to(&path) {
            self.append(&format!("Save failed. The previous level file is unchanged. {error}"));
        }
    }

    pub(super) fn file_save_as(&mut self) {
        let Some(path) = save_dialog(self.frame, "Save JARVIG Level", "JARVIG Level\0*.jarviglevel\0", "jarviglevel") else { return };
        let path = ensure_extension(&path, "jarviglevel");
        if let Err(error) = self.save_level_to(&path) {
            self.append(&format!("Save As failed. The previous level file is unchanged. {error}"));
        }
    }

    pub(super) fn file_save_all(&mut self) {
        self.file_save();
        if let (Some(path), Some(project)) = (self.project_file.clone(), self.project_document()) {
            let backup = backup_dir(&path);
            if let Err(error) = save_project_atomic(&path, &backup, &project) {
                self.append(&format!("Project file was not rewritten: {error}"));
            }
        }
    }

    pub(super) fn file_recent_project(&mut self, index: usize) {
        self.stop_play();
        let Some(path) = self.recent_projects.get(index).cloned() else { return };
        if !self.confirm_save_or_discard() {
            return;
        }
        if let Err(error) = self.open_project_at(&path) {
            self.append(&format!("Recent project did not open: {error}"));
        }
    }

    pub(super) fn file_recent_level(&mut self, index: usize) {
        self.stop_play();
        let Some(path) = self.recent_levels.get(index).cloned() else { return };
        if !self.confirm_save_or_discard() {
            return;
        }
        if let Err(error) = self.load_level_from(&path, &path.display().to_string()) {
            self.fail_progress("Loading level file", &error);
            self.append(&format!("Recent level was rejected. The current world is unchanged. {error}"));
        }
    }

    pub(super) fn note_policy_edit(&mut self) {
        if self.self_test || self.level_name.is_empty() {
            return;
        }
        self.unsaved_policy = true;
        self.refresh_title();
    }

    pub(super) fn restore_editor_viewport(&mut self) {
        let Some(path) = self.viewport_path() else { return };
        let Ok(text) = fs::read_to_string(path) else { return };
        // A bad viewport file must not touch the level.
        let Some(restored) = parse_viewport(&text) else { return };
        if let Some(controller) = self.editor_camera.as_mut() {
            controller.position = restored.0;
            controller.yaw = restored.1;
            controller.pitch = restored.2;
            controller.speed_m_s = restored.3;
        }
    }

    pub(super) fn file_reload_level(&mut self) {
        self.stop_play();
        if !self.confirm_save_or_discard() {
            return;
        }
        let Some(path) = self.level_file.clone() else {
            self.append("Reload needs an open level file. The world was not changed.");
            return;
        };
        if let Err(error) = self.load_level_from(&path, "Reloading the open level.") {
            self.fail_progress("Loading level file", &error);
        }
    }

    fn load_level_from(&mut self, path: &Path, detail: &str) -> Result<(), String> {
        let outer = self.load_depth == 0;
        if outer {
            self.begin_load();
        }
        self.report("Loading level file", detail, None);
        self.pump_loading();
        let loaded = load_level_file(path).map_err(|error| format!("the level file was not read. The current world is unchanged. {error}"));
        let result = match loaded {
            Ok(document) => match self.adopt_level(document, Some(path.to_path_buf())) {
                Ok(()) => {
                    self.restore_workspace();
                    Ok(())
                }
                Err(error) => Err(error),
            },
            Err(error) => Err(error),
        };
        if outer {
            self.end_load();
        }
        result
    }

    fn open_project_at(&mut self, path: &Path) -> Result<(), String> {
        let outer = self.load_depth == 0;
        if outer {
            self.begin_load();
        }
        self.report("Opening project", &path.display().to_string(), None);
        self.pump_loading();
        let result = (|| {
            let project = load_project_file(path).map_err(|error| error.to_string())?;
            let level_path = project.startup_level_path(path).map_err(|error| error.to_string())?;
            self.report("Loading level file", &level_path.display().to_string(), None);
            self.pump_loading();
            let document = load_level_file(&level_path).map_err(|error| format!("startup level was not loaded. The current world is unchanged. {error}"))?;
            self.project_file = Some(path.to_path_buf());
            self.project_name = project.display_name.clone();
            self.adopt_level(document, Some(level_path))?;
            let templated_land = self.apply_saved_template();
            if !self.restore_workspace() && templated_land {
                self.append(Self::workspace_line(super::chrome::WorkspaceMode::Land));
            }
            self.remember_project(path);
            self.append(&format!("Project {} is open. Startup level is {}.", project.display_name, project.startup_level));
            self.queue_asset_registry();
            Ok(())
        })();
        if outer {
            self.end_load();
        }
        result
    }

    fn adopt_level(&mut self, document: LevelDocument, path: Option<PathBuf>) -> Result<(), String> {
        let name = document.name.clone();
        let uuid = document.level_uuid;
        if let Some(project_file) = self.project_file.clone() {
            let project = load_project_file(&project_file).map_err(|error| error.to_string())?;
            let root = project_file.parent().ok_or("project file has no directory")?;
            self.report("Loading derived meshes", "Canonical meshes and stored meshlets.", None);
            self.pump_loading();
            let assets = jarvig_core::load_project_mesh_assets(root, &project.content_directory, &project.intermediate_directory)?;
            self.note_loaded_meshlets(&assets);
            self.engine.replace_mesh_assets(assets.clone());
            self.game.set_mesh_assets(assets);
        }
        if let Err(error) = self.install_staged_materials(&document) {
            self.fail_progress("Loading materials", &error);
            return Err(error);
        }
        self.report("Creating entities", &name, None);
        self.pump_loading();
        if let Err(error) = self.engine.load_level(&document) {
            let error = error.to_string();
            self.fail_progress("Creating entities", &error);
            return Err(error);
        }
        self.level_file = path.clone();
        self.level_name = name;
        self.level_uuid = Some(uuid);
        self.unsaved_policy = false;
        self.saved_revision = self.engine.world().revision();
        self.last_autosave = Instant::now();
        if let Some(controller) = self.editor_camera.as_mut() {
            controller.reference_frame = self.engine.front_camera().frame;
        }
        let _ = self.push_editor_camera();
        self.selection.clear_from(crate::selection::SelectionSource::Outliner);
        self.selection_view_ready = false;
        self.character_workspace = false;
        self.land_mode = false;
        self.outliner_land = false;
        self.cancel_land_chunks();
        self.sync_outliner();
        if let Some(path) = path {
            self.remember_level(&path);
        }
        self.append(&format!("Level {} is the authored world. The editor camera was not loaded from the level.", self.level_name));
        self.refresh_title();
        Ok(())
    }

    /// Loads one character asset into the open world and enters the Character Editor.
    ///
    /// The character file stays put. Save still writes a `.jarviglevel`.
    pub(super) fn open_character_document(&mut self, asset: &RegistryAsset) -> Result<(), String> {
        let project = self.project_file.clone().ok_or("Open a project before opening a character.")?;
        let root = project.parent().ok_or("project file has no directory")?;
        let path = root.join(&asset.path);
        let text = fs::read_to_string(&path).map_err(|error| format!("character file was not read: {error}"))?;
        let character = parse_character(&text).map_err(|error| error.to_string())?;
        let (level_uuid, settings_uuid) = if character.name == "Base Male" {
            (
                EntityId::parse("33333333-3333-4333-8333-3333333333b2").expect("male level uuid"),
                EntityId::parse("33333333-3333-4333-8333-3333333333a2").expect("male settings uuid"),
            )
        } else {
            (
                EntityId::parse("33333333-3333-4333-8333-3333333333b1").expect("base level uuid"),
                EntityId::parse("33333333-3333-4333-8333-3333333333a1").expect("base settings uuid"),
            )
        };
        let document = character.open_as_level(level_uuid, settings_uuid).map_err(|error| error.to_string())?;
        self.adopt_level(document, None)?;
        self.set_character_workspace(true);
        self.focus_character_body();
        self.append(&format!("Opened {}. Double-click did not place a copy. Save writes a level, not the character file.", character.name));
        Ok(())
    }

    fn save_level_to(&mut self, path: &Path) -> Result<(), String> {
        let uuid = self.level_uuid.unwrap_or_else(EntityId::new);
        let name = if self.level_name.is_empty() {
            path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("Level").to_string()
        } else {
            self.level_name.clone()
        };
        let document = self.engine.export_level(uuid, &name)?;
        self.store_level(path, &document, false)?;
        self.level_file = Some(path.to_path_buf());
        self.level_name = document.name;
        self.level_uuid = Some(document.level_uuid);
        self.unsaved_policy = false;
        self.saved_revision = self.engine.world().revision();
        self.remember_level(path);
        self.write_editor_viewport();
        self.persist_workspace();
        self.append(&format!("Saved {}. Runtime ids and the editor camera were not written into the level.", path.display()));
        self.refresh_title();
        Ok(())
    }

    fn store_level(&mut self, path: &Path, document: &LevelDocument, creating: bool) -> Result<(), String> {
        let backup = self.backup_dir_for(path);
        save_level_atomic(path, &backup, document).map_err(|error| error.to_string())?;
        if creating {
            self.append(&format!("Wrote {}.", path.display()));
        }
        Ok(())
    }

    fn confirm_save_or_discard(&mut self) -> bool {
        if !self.level_dirty() {
            return true;
        }
        let text = wide("This level has unsaved changes. Save before continuing?");
        let title = wide("JARVIGEditor");
        let answer = unsafe { MessageBoxW(self.frame, text.as_ptr(), title.as_ptr(), MB_YESNOCANCEL) };
        match answer {
            IDYES => {
                self.file_save();
                !self.level_dirty()
            }
            IDNO => true,
            IDCANCEL => false,
            _ => false,
        }
    }

    fn project_document(&self) -> Option<ProjectDocument> {
        self.project_file.as_ref().and_then(|path| load_project_file(path).ok())
    }

    fn backup_dir_for(&self, level: &Path) -> PathBuf {
        if let Some(project) = &self.project_file {
            if let Ok(document) = load_project_file(project) {
                if let Ok(path) = document.directory(project, "Saved/Backup") {
                    return path;
                }
            }
        }
        level.parent().unwrap_or(Path::new(".")).join("Backup")
    }

    fn viewport_path(&self) -> Option<PathBuf> {
        let project = self.project_file.as_ref()?;
        let document = load_project_file(project).ok()?;
        document.directory(project, "Saved/Editor/viewport.json").ok()
    }

    fn workspace_path(&self) -> Option<PathBuf> {
        let project = self.project_file.as_ref()?;
        let document = load_project_file(project).ok()?;
        document.directory(project, "Saved/Editor/workspace.json").ok()
    }

    /// Last editor mode and its visualization. Not level data. A self-test writes nothing.
    pub(super) fn persist_workspace(&self) {
        if self.self_test || self.project_file.is_none() {
            return;
        }
        let Some(path) = self.workspace_path() else { return };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, format_workspace(&EditorWorkspace::capture(self)));
    }

    /// A saved workspace wins over the project template. No file leaves the mode adopt just set.
    fn restore_workspace(&mut self) -> bool {
        let Some(path) = self.workspace_path() else { return false };
        let Ok(text) = fs::read_to_string(path) else { return false };
        let Some(saved) = parse_workspace(&text) else {
            self.append("Editor workspace file was left unread. The mode stayed.");
            return false;
        };
        if saved.mode == super::chrome::WorkspaceMode::Character && !self.level_has_rig() {
            self.apply_workspace(&saved);
            if self.editor_mode() != super::chrome::WorkspaceMode::Level {
                self.switch_editor_mode(super::chrome::WorkspaceMode::Level, false, false);
            }
            self.finish_workspace_restore();
            self.append("Workspace kept Level. This level has no rig.");
            return true;
        }
        if self.editor_mode() != saved.mode {
            self.switch_editor_mode(saved.mode, false, false);
        }
        self.apply_workspace(&saved);
        self.finish_workspace_restore();
        let name = match saved.mode {
            super::chrome::WorkspaceMode::Level => "Level",
            super::chrome::WorkspaceMode::Land => "Land",
            super::chrome::WorkspaceMode::Character => "Character",
        };
        self.append(&format!("Workspace restored: {name}."));
        true
    }

    fn apply_workspace(&mut self, saved: &EditorWorkspace) {
        self.land_grid = saved.grid;
        self.land_grid_minor = saved.minor;
        self.land_grid_major = saved.major.max(saved.minor);
        self.land_grid_snap = saved.snap;
        self.land_overlay_world = saved.world;
        self.land_overlay_vertices = saved.vertices;
        self.land_overlay_chunks = saved.chunks;
        self.land_overlay_lod = saved.lod;
        self.land_show_terrain = saved.show_terrain;
        self.land_show_helpers = saved.show_helpers;
        self.land_show_lighting = saved.show_lighting;
        self.land_show_characters = saved.show_characters;
        self.land_show_props = saved.show_props;
        self.land_show_gameplay = saved.show_gameplay;
        self.land_show_full = saved.show_full;
        self.land_radius = saved.radius.clamp(0.25, 128.0);
        self.land_delta = saved.strength.max(0.0);
        self.land_falloff = saved.falloff;
        self.land_tool = saved.tool;
        self.show_joint_debug = saved.joints;
        self.show_all_joints = saved.all_joints && saved.joints;
        self.show_joint_limits = saved.limits;
    }

    fn finish_workspace_restore(&mut self) {
        self.inspector_force_realize = true;
        self.request_inspector_refresh();
        self.sync_view_menu();
    }

    fn write_editor_viewport(&self) {
        let Some(path) = self.viewport_path() else { return };
        let Some(controller) = self.editor_camera.as_ref() else { return };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let position = controller.position;
        let text = format!(
            "{{\n  \"schema\": \"jarvig.editor-viewport\",\n  \"format_version\": 1,\n  \"position\": [{}, {}, {}],\n  \"yaw\": {},\n  \"pitch\": {},\n  \"speed_m_s\": {}\n}}\n",
            position.x, position.y, position.z, controller.yaw, controller.pitch, controller.speed_m_s
        );
        let _ = fs::write(path, text);
    }

    /// A Terrain World opens in Land when no workspace file has been saved yet.
    /// The call does not write `workspace.json`, so a later restore still wins.
    fn apply_saved_template(&mut self) -> bool {
        let Some(word) = self.read_project_template() else { return false };
        match word.as_str() {
            "terrain" | "land" => {
                self.switch_editor_mode(super::chrome::WorkspaceMode::Land, false, false);
                true
            }
            "third-person" | "fps" => {
                self.append("This template is an empty world. A pawn is not in this foundation.");
                false
            }
            _ => false,
        }
    }

    fn read_project_template(&self) -> Option<String> {
        let project_file = self.project_file.as_ref()?;
        let project = load_project_file(project_file).ok()?;
        let path = project.directory(project_file, "Saved/Editor/template.txt").ok()?;
        let text = fs::read_to_string(path).ok()?;
        let word = text.lines().next().unwrap_or("").trim().to_ascii_lowercase();
        if word.is_empty() { None } else { Some(word) }
    }

    fn remember_project(&mut self, path: &Path) {
        remember(&mut self.recent_projects, path);
        self.persist_recent();
        self.rebuild_recent_menus();
    }

    fn remember_level(&mut self, path: &Path) {
        remember(&mut self.recent_levels, path);
        self.persist_recent();
        self.rebuild_recent_menus();
    }

    fn rebuild_recent_menus(&self) {
        unsafe {
            fill_menu(self.recent_project_menu, &self.recent_projects, super::ID_FILE_RECENT_PROJECT);
            fill_menu(self.recent_level_menu, &self.recent_levels, super::ID_FILE_RECENT_LEVEL);
        }
    }

    fn persist_recent(&self) {
        let Some(project) = &self.project_file else { return };
        let Ok(document) = load_project_file(project) else { return };
        let Ok(directory) = document.directory(project, "Saved/Editor") else { return };
        let _ = fs::create_dir_all(&directory);
        let mut text = String::from("projects\n");
        for path in &self.recent_projects {
            text.push_str(&path.display().to_string());
            text.push('\n');
        }
        text.push_str("levels\n");
        for path in &self.recent_levels {
            text.push_str(&path.display().to_string());
            text.push('\n');
        }
        let _ = fs::write(directory.join("recent.txt"), text);
    }
}

fn write_template_file(project_path: &Path, project: &ProjectDocument, word: &str) {
    let Ok(directory) = project.directory(project_path, "Saved/Editor") else { return };
    let _ = fs::create_dir_all(&directory);
    let _ = fs::write(directory.join("template.txt"), format!("{word}\n"));
}

fn backup_dir(project_file: &Path) -> PathBuf {
    project_file.parent().unwrap_or(Path::new(".")).join("Saved/Backup")
}

unsafe fn fill_menu(menu: windows_sys::Win32::UI::WindowsAndMessaging::HMENU, paths: &[PathBuf], base: usize) {
    if menu.is_null() {
        return;
    }
    while windows_sys::Win32::UI::WindowsAndMessaging::GetMenuItemCount(menu) > 0 {
        windows_sys::Win32::UI::WindowsAndMessaging::DeleteMenu(menu, 0, windows_sys::Win32::UI::WindowsAndMessaging::MF_BYPOSITION);
    }
    if paths.is_empty() {
        super::append(menu, base + 64, "(none)");
        return;
    }
    for (index, path) in paths.iter().enumerate().take(8) {
        super::append(menu, base + index, &path.display().to_string());
    }
}

fn remember(list: &mut Vec<PathBuf>, path: &Path) {
    list.retain(|stored| stored != path);
    list.insert(0, path.to_path_buf());
    list.truncate(8);
}

fn ensure_extension(path: &Path, extension: &str) -> PathBuf {
    if path.extension().and_then(|ext| ext.to_str()) == Some(extension) {
        path.to_path_buf()
    } else {
        let mut name = path.as_os_str().to_os_string();
        name.push(".");
        name.push(extension);
        PathBuf::from(name)
    }
}

fn find_up(relative: &str) -> Option<PathBuf> {
    let mut starts = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            starts.push(parent.to_path_buf());
        }
    }
    for mut dir in starts {
        for _ in 0..8 {
            let candidate = dir.join(relative);
            if candidate.is_file() {
                return Some(candidate);
            }
            if !dir.pop() {
                break;
            }
        }
    }
    None
}

fn parse_viewport(text: &str) -> Option<(Vec3, f64, f64, f64)> {
    let position = array_after(text, "\"position\"")?;
    let yaw = number_after(text, "\"yaw\"")?;
    let pitch = number_after(text, "\"pitch\"")?;
    let speed = number_after(text, "\"speed_m_s\"")?;
    if position.len() != 3 || !yaw.is_finite() || !pitch.is_finite() || !speed.is_finite() {
        return None;
    }
    Some((Vec3::new(position[0], position[1], position[2]), yaw, pitch, speed))
}

fn array_after(text: &str, key: &str) -> Option<Vec<f64>> {
    let start = text.find(key)? + key.len();
    let slice = &text[start..];
    let open = slice.find('[')?;
    let close = slice[open..].find(']')?;
    let body = &slice[open + 1..open + close];
    let values: Vec<f64> = body.split(',').filter_map(|part| part.trim().parse().ok()).collect();
    Some(values)
}

fn number_after(text: &str, key: &str) -> Option<f64> {
    let start = text.find(key)? + key.len();
    let slice = text[start..].trim_start_matches(|character: char| !character.is_ascii_digit() && character != '-' && character != '.');
    let end = slice.find(|character: char| !(character.is_ascii_digit() || character == '-' || character == '.' || character == 'e' || character == 'E' || character == '+')).unwrap_or(slice.len());
    slice[..end].parse().ok()
}

fn word_after(text: &str, key: &str) -> Option<String> {
    let start = text.find(key)? + key.len();
    let slice = text[start..].trim_start_matches(|character: char| character == ':' || character.is_whitespace());
    if let Some(rest) = slice.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(rest[..end].to_string());
    }
    let end = slice.find(|character: char| character == ',' || character == '\n' || character == '\r' || character == '}').unwrap_or(slice.len());
    let word = slice[..end].trim();
    if word.is_empty() { None } else { Some(word.to_string()) }
}

fn flag_after(text: &str, key: &str) -> Option<bool> {
    match word_after(text, key)?.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn json_bool(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

/// Editor view over one project. The level file does not store this.
#[derive(Clone, Copy, Debug, PartialEq)]
struct EditorWorkspace {
    mode: super::chrome::WorkspaceMode,
    grid: bool,
    minor: f32,
    major: f32,
    snap: bool,
    world: bool,
    vertices: bool,
    chunks: bool,
    lod: bool,
    show_terrain: bool,
    show_helpers: bool,
    show_lighting: bool,
    show_characters: bool,
    show_props: bool,
    show_gameplay: bool,
    show_full: bool,
    radius: f32,
    strength: f32,
    falloff: jarvig_core::TerrainFalloff,
    tool: super::LandTool,
    joints: bool,
    all_joints: bool,
    limits: bool,
}

impl EditorWorkspace {
    fn capture(editor: &Editor) -> Self {
        Self {
            mode: editor.editor_mode(),
            grid: editor.land_grid,
            minor: editor.land_grid_minor,
            major: editor.land_grid_major,
            snap: editor.land_grid_snap,
            world: editor.land_overlay_world,
            vertices: editor.land_overlay_vertices,
            chunks: editor.land_overlay_chunks,
            lod: editor.land_overlay_lod,
            show_terrain: editor.land_show_terrain,
            show_helpers: editor.land_show_helpers,
            show_lighting: editor.land_show_lighting,
            show_characters: editor.land_show_characters,
            show_props: editor.land_show_props,
            show_gameplay: editor.land_show_gameplay,
            show_full: editor.land_show_full,
            radius: editor.land_radius,
            strength: editor.land_delta,
            falloff: editor.land_falloff,
            tool: editor.land_tool,
            joints: editor.show_joint_debug,
            all_joints: editor.show_all_joints,
            limits: editor.show_joint_limits,
        }
    }
}

fn format_workspace(saved: &EditorWorkspace) -> String {
    let mode = match saved.mode {
        super::chrome::WorkspaceMode::Level => "level",
        super::chrome::WorkspaceMode::Land => "land",
        super::chrome::WorkspaceMode::Character => "character",
    };
    format!(
        "{{\n  \"schema\": \"jarvig.editor-workspace\",\n  \"format_version\": 1,\n  \"mode\": \"{mode}\",\n  \"grid\": {grid},\n  \"minor\": {minor},\n  \"major\": {major},\n  \"snap\": {snap},\n  \"world\": {world},\n  \"vertices\": {vertices},\n  \"chunks\": {chunks},\n  \"lod\": {lod},\n  \"show_terrain\": {show_terrain},\n  \"show_helpers\": {show_helpers},\n  \"show_lighting\": {show_lighting},\n  \"show_characters\": {show_characters},\n  \"show_props\": {show_props},\n  \"show_gameplay\": {show_gameplay},\n  \"show_full\": {show_full},\n  \"radius\": {radius},\n  \"strength\": {strength},\n  \"falloff\": \"{falloff}\",\n  \"tool\": \"{tool}\",\n  \"joints\": {joints},\n  \"all_joints\": {all_joints},\n  \"limits\": {limits}\n}}\n",
        grid = json_bool(saved.grid),
        minor = saved.minor,
        major = saved.major,
        snap = json_bool(saved.snap),
        world = json_bool(saved.world),
        vertices = json_bool(saved.vertices),
        chunks = json_bool(saved.chunks),
        lod = json_bool(saved.lod),
        show_terrain = json_bool(saved.show_terrain),
        show_helpers = json_bool(saved.show_helpers),
        show_lighting = json_bool(saved.show_lighting),
        show_characters = json_bool(saved.show_characters),
        show_props = json_bool(saved.show_props),
        show_gameplay = json_bool(saved.show_gameplay),
        show_full = json_bool(saved.show_full),
        radius = saved.radius,
        strength = saved.strength,
        falloff = saved.falloff.label(),
        tool = saved.tool.key(),
        joints = json_bool(saved.joints),
        all_joints = json_bool(saved.all_joints),
        limits = json_bool(saved.limits),
    )
}

fn parse_workspace(text: &str) -> Option<EditorWorkspace> {
    if word_after(text, "\"schema\"")?.as_str() != "jarvig.editor-workspace" {
        return None;
    }
    if number_after(text, "\"format_version\"")? != 1.0 {
        return None;
    }
    let mode = match word_after(text, "\"mode\"")?.as_str() {
        "level" => super::chrome::WorkspaceMode::Level,
        "land" => super::chrome::WorkspaceMode::Land,
        "character" => super::chrome::WorkspaceMode::Character,
        _ => return None,
    };
    let minor = number_after(text, "\"minor\"")? as f32;
    let major = number_after(text, "\"major\"")? as f32;
    let radius = number_after(text, "\"radius\"")? as f32;
    let strength = number_after(text, "\"strength\"")? as f32;
    if !minor.is_finite() || !major.is_finite() || !radius.is_finite() || !strength.is_finite() {
        return None;
    }
    if minor <= 0.0 || major <= 0.0 || radius <= 0.0 || strength < 0.0 {
        return None;
    }
    let falloff = jarvig_core::TerrainFalloff::parse(&word_after(text, "\"falloff\"")?)?;
    let tool = super::LandTool::from_key(&word_after(text, "\"tool\"")?)?;
    Some(EditorWorkspace {
        mode,
        grid: flag_after(text, "\"grid\"")?,
        minor,
        major,
        snap: flag_after(text, "\"snap\"")?,
        world: flag_after(text, "\"world\"")?,
        vertices: flag_after(text, "\"vertices\"")?,
        chunks: flag_after(text, "\"chunks\"")?,
        lod: flag_after(text, "\"lod\"")?,
        show_terrain: flag_after(text, "\"show_terrain\"")?,
        show_helpers: flag_after(text, "\"show_helpers\"")?,
        show_lighting: flag_after(text, "\"show_lighting\"")?,
        show_characters: flag_after(text, "\"show_characters\"")?,
        show_props: flag_after(text, "\"show_props\"")?,
        show_gameplay: flag_after(text, "\"show_gameplay\"")?,
        show_full: flag_after(text, "\"show_full\"")?,
        radius,
        strength,
        falloff,
        tool,
        joints: flag_after(text, "\"joints\"")?,
        all_joints: flag_after(text, "\"all_joints\"")?,
        limits: flag_after(text, "\"limits\"")?,
    })
}

fn open_dialog(owner: windows_sys::Win32::Foundation::HWND, title: &str, filter: &str) -> Option<PathBuf> {
    dialog(owner, title, filter, None, true)
}

fn save_dialog(owner: windows_sys::Win32::Foundation::HWND, title: &str, filter: &str, extension: &str) -> Option<PathBuf> {
    dialog(owner, title, filter, Some(extension), false)
}

fn dialog(owner: windows_sys::Win32::Foundation::HWND, title: &str, filter: &str, extension: Option<&str>, open: bool) -> Option<PathBuf> {
    let mut file = [0u16; 520];
    let filter = wide(filter);
    let title = wide(title);
    let default_ext = extension.map(wide);
    let mut info: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    info.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    info.hwndOwner = owner;
    info.lpstrFilter = filter.as_ptr();
    info.lpstrFile = file.as_mut_ptr();
    info.nMaxFile = file.len() as u32;
    info.lpstrTitle = title.as_ptr();
    info.Flags = if open { OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST } else { OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST };
    info.lpstrDefExt = default_ext.as_ref().map(|text| text.as_ptr()).unwrap_or(std::ptr::null());
    let chosen = unsafe { if open { GetOpenFileNameW(&mut info) } else { GetSaveFileNameW(&mut info) } };
    if chosen == 0 {
        return None;
    }
    let length = file.iter().position(|unit| *unit == 0).unwrap_or(file.len());
    Some(PathBuf::from(String::from_utf16_lossy(&file[..length])))
}

#[cfg(test)]
mod tests {
    use super::{format_workspace, parse_workspace, EditorWorkspace};

    #[test]
    fn workspace_file_roundtrips_and_rejects_a_bad_schema() {
        let saved = EditorWorkspace {
            mode: crate::chrome::WorkspaceMode::Land,
            grid: true,
            minor: 1.0,
            major: 10.0,
            snap: false,
            world: true,
            vertices: false,
            chunks: false,
            lod: false,
            show_terrain: true,
            show_helpers: true,
            show_lighting: true,
            show_characters: false,
            show_props: false,
            show_gameplay: false,
            show_full: false,
            radius: 4.0,
            strength: 0.35,
            falloff: jarvig_core::TerrainFalloff::Smooth,
            tool: crate::LandTool::Sculpt,
            joints: true,
            all_joints: false,
            limits: true,
        };
        let text = format_workspace(&saved);
        let parsed = parse_workspace(&text).expect("workspace");
        assert_eq!(parsed.mode, saved.mode);
        assert_eq!(parsed.tool, saved.tool);
        assert_eq!(parsed.falloff, saved.falloff);
        assert_eq!(parsed.grid, true);
        assert_eq!(parsed.snap, false);
        assert_eq!(parsed.show_characters, false);
        assert_eq!(parsed.show_full, false);
        assert_eq!(parsed.joints, true);
        assert_eq!(parsed.all_joints, false);
        assert_eq!(parsed.limits, true);
        assert!((parsed.minor - saved.minor).abs() < 1.0e-5);
        assert!((parsed.major - saved.major).abs() < 1.0e-5);
        assert!((parsed.radius - saved.radius).abs() < 1.0e-5);
        assert!((parsed.strength - saved.strength).abs() < 1.0e-5);
        assert!(parse_workspace(&text.replace("jarvig.editor-workspace", "jarvig.other")).is_none());
        assert!(parse_workspace(&text.replace("\"tool\": \"sculpt\"", "\"tool\": \"nope\"")).is_none());
        assert!(parse_workspace(&text.replace("  \"mode\": \"land\",\n", "")).is_none());
    }
}
