//! Project descriptor. Paths inside it are project-relative. They are not asset identities
//! and they are not absolute filesystem paths.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::json_lite::{parse_json, Json, JsonError};
use crate::level::{parse_level, LevelDocument};
use crate::EntityId;

pub const PROJECT_SCHEMA: &str = "jarvig.project";
pub const PROJECT_FORMAT_VERSION: u32 = 1;
pub const LIGHTING_LAB_PROJECT_UUID: &str = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectDocument {
    pub format_version: u32,
    pub project_uuid: EntityId,
    pub display_name: String,
    pub engine_version: String,
    pub startup_level: String,
    pub content_directory: String,
    pub saved_directory: String,
    pub config_directory: String,
    pub intermediate_directory: String,
    pub settings: String,
}

#[derive(Debug)]
pub enum ProjectError {
    Syntax(String),
    UnsupportedVersion(u32),
    AbsolutePath(String),
    Corrupt(String),
    Io(String),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syntax(text) | Self::Corrupt(text) | Self::Io(text) | Self::AbsolutePath(text) => write!(formatter, "{text}"),
            Self::UnsupportedVersion(version) => write!(formatter, "project format version {version} is newer than {PROJECT_FORMAT_VERSION}"),
        }
    }
}

impl From<JsonError> for ProjectError {
    fn from(error: JsonError) -> Self {
        Self::Syntax(error.0)
    }
}

impl ProjectDocument {
    pub fn lighting_lab() -> Self {
        Self {
            format_version: PROJECT_FORMAT_VERSION,
            project_uuid: EntityId::parse(LIGHTING_LAB_PROJECT_UUID).expect("project uuid"),
            display_name: "Lighting Lab".into(),
            engine_version: env!("CARGO_PKG_VERSION").into(),
            startup_level: "Content/Levels/LightingLab.jarviglevel".into(),
            content_directory: "Content".into(),
            saved_directory: "Saved".into(),
            config_directory: "Config".into(),
            intermediate_directory: "Intermediate".into(),
            settings: "Config/Project.jarvigsettings".into(),
        }
    }

    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.format_version == 0 || self.display_name.is_empty() || self.engine_version.is_empty() || !self.project_uuid.is_persistent() {
            return Err(ProjectError::Corrupt("project identity is incomplete".into()));
        }
        if self.format_version > PROJECT_FORMAT_VERSION {
            return Err(ProjectError::UnsupportedVersion(self.format_version));
        }
        for path in [
            &self.startup_level,
            &self.content_directory,
            &self.saved_directory,
            &self.config_directory,
            &self.intermediate_directory,
            &self.settings,
        ] {
            reject_absolute(path)?;
        }
        Ok(())
    }

    pub fn to_json(&self) -> String {
        Json::object(vec![
            ("schema", Json::string(PROJECT_SCHEMA)),
            ("format_version", Json::int(self.format_version as i64)),
            ("project_uuid", Json::string(self.project_uuid.to_string())),
            ("display_name", Json::string(&self.display_name)),
            ("engine_version", Json::string(&self.engine_version)),
            ("startup_level", Json::string(&self.startup_level)),
            ("content_directory", Json::string(&self.content_directory)),
            ("saved_directory", Json::string(&self.saved_directory)),
            ("config_directory", Json::string(&self.config_directory)),
            ("intermediate_directory", Json::string(&self.intermediate_directory)),
            ("settings", Json::string(&self.settings)),
        ])
        .write()
    }

    pub fn startup_level_path(&self, project_file: &Path) -> Result<PathBuf, ProjectError> {
        self.validate()?;
        let root = project_root(project_file)?;
        Ok(root.join(&self.startup_level))
    }

    pub fn directory(&self, project_file: &Path, relative: &str) -> Result<PathBuf, ProjectError> {
        reject_absolute(relative)?;
        Ok(project_root(project_file)?.join(relative))
    }
}

pub fn parse_project(text: &str) -> Result<ProjectDocument, ProjectError> {
    let json = parse_json(text)?;
    let schema = required_str(&json, "schema")?;
    if schema != PROJECT_SCHEMA {
        return Err(ProjectError::Corrupt(format!("schema {schema} is not {PROJECT_SCHEMA}")));
    }
    let format_version = required_u32(&json, "format_version")?;
    if format_version > PROJECT_FORMAT_VERSION {
        return Err(ProjectError::UnsupportedVersion(format_version));
    }
    let document = ProjectDocument {
        format_version,
        project_uuid: EntityId::parse(required_str(&json, "project_uuid")?).ok_or_else(|| ProjectError::Corrupt("project uuid is invalid".into()))?,
        display_name: required_str(&json, "display_name")?.to_string(),
        engine_version: required_str(&json, "engine_version")?.to_string(),
        startup_level: required_str(&json, "startup_level")?.to_string(),
        content_directory: required_str(&json, "content_directory")?.to_string(),
        saved_directory: required_str(&json, "saved_directory")?.to_string(),
        config_directory: required_str(&json, "config_directory")?.to_string(),
        intermediate_directory: required_str(&json, "intermediate_directory")?.to_string(),
        settings: required_str(&json, "settings")?.to_string(),
    };
    document.validate()?;
    Ok(document)
}

pub fn project_root(project_file: &Path) -> Result<PathBuf, ProjectError> {
    project_file.parent().map(Path::to_path_buf).filter(|path| !path.as_os_str().is_empty()).ok_or_else(|| ProjectError::Corrupt("project file has no directory".into()))
}

/// Content, Config, Saved, and Intermediate. Does not write a level.
pub fn create_project_directories(project_file: &Path, project: &ProjectDocument) -> Result<(), ProjectError> {
    project.validate()?;
    let root = project_root(project_file)?;
    for relative in [
        project.content_directory.as_str(),
        "Content/Levels",
        "Content/Materials",
        "Content/Meshes",
        "Content/Textures",
        project.config_directory.as_str(),
        project.saved_directory.as_str(),
        "Saved/Autosaves",
        "Saved/Backup",
        "Saved/Logs",
        "Saved/Editor",
        project.intermediate_directory.as_str(),
    ] {
        fs::create_dir_all(root.join(relative)).map_err(|error| ProjectError::Io(error.to_string()))?;
    }
    Ok(())
}

/// Write bytes to a temp file in the destination directory, fsync, copy the previous file
/// into `backup_dir`, then replace. A failure before the replace leaves the destination intact.
pub fn save_atomic(destination: &Path, backup_dir: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| ProjectError::Io(error.to_string()))?;
    }
    fs::create_dir_all(backup_dir).map_err(|error| ProjectError::Io(error.to_string()))?;
    let mut temp_name = destination.file_name().unwrap_or_default().to_os_string();
    temp_name.push(".tmp");
    let temp = destination.with_file_name(temp_name);
    {
        let mut file = File::create(&temp).map_err(|error| ProjectError::Io(error.to_string()))?;
        file.write_all(bytes).map_err(|error| ProjectError::Io(error.to_string()))?;
        file.sync_all().map_err(|error| ProjectError::Io(error.to_string()))?;
    }
    let read_back = fs::read(&temp).map_err(|error| ProjectError::Io(error.to_string()))?;
    if read_back != bytes {
        let _ = fs::remove_file(&temp);
        return Err(ProjectError::Io("temporary save did not match the bytes just written".into()));
    }
    if destination.exists() {
        let backup_name = destination.file_name().unwrap_or_default();
        let backup = backup_dir.join(backup_name);
        fs::copy(destination, &backup).map_err(|error| ProjectError::Io(error.to_string()))?;
    }
    if destination.exists() {
        fs::remove_file(destination).map_err(|error| ProjectError::Io(error.to_string()))?;
    }
    if let Err(error) = fs::rename(&temp, destination) {
        return Err(ProjectError::Io(format!("replace failed after the previous file was backed up: {error}")));
    }
    Ok(())
}

pub fn save_level_atomic(destination: &Path, backup_dir: &Path, document: &LevelDocument) -> Result<(), ProjectError> {
    document.validate().map_err(|error| ProjectError::Corrupt(error.to_string()))?;
    let text = document.to_json();
    parse_level_text(&text).map_err(|error| ProjectError::Corrupt(error.to_string()))?;
    save_atomic(destination, backup_dir, text.as_bytes())
}

pub fn save_project_atomic(destination: &Path, backup_dir: &Path, document: &ProjectDocument) -> Result<(), ProjectError> {
    document.validate()?;
    let text = document.to_json();
    parse_project(&text)?;
    save_atomic(destination, backup_dir, text.as_bytes())
}

pub fn load_level_file(path: &Path) -> Result<LevelDocument, ProjectError> {
    let text = fs::read_to_string(path).map_err(|error| ProjectError::Io(error.to_string()))?;
    parse_level_text(&text).map_err(|error| ProjectError::Corrupt(error.to_string()))
}

/// Writes the regression project. The level bytes come from [`crate::lighting_lab_level`], not from a hand-edited copy.
#[allow(dead_code)]
pub fn write_lighting_lab_sample(root: &Path) -> Result<(), ProjectError> {
    let project_file = root.join("LightingLab.jarvigproject");
    let project = ProjectDocument::lighting_lab();
    create_project_directories(&project_file, &project)?;
    let backup = root.join("Saved/Backup");
    save_project_atomic(&project_file, &backup, &project)?;
    let level = crate::lighting_lab_level();
    save_level_atomic(&project.startup_level_path(&project_file)?, &backup, &level)?;
    let settings = root.join(&project.settings);
    if let Some(parent) = settings.parent() {
        fs::create_dir_all(parent).map_err(|error| ProjectError::Io(error.to_string()))?;
    }
    fs::write(settings, "JARVIG project settings placeholder. This is not a level and not a GPU resource.\n").map_err(|error| ProjectError::Io(error.to_string()))?;
    Ok(())
}

pub fn load_project_file(path: &Path) -> Result<ProjectDocument, ProjectError> {
    let text = fs::read_to_string(path).map_err(|error| ProjectError::Io(error.to_string()))?;
    parse_project(&text)
}

/// Autosave name. This is never the authored `.jarviglevel` path.
pub fn autosave_path(project_file: &Path, project: &ProjectDocument, level_file_name: &str) -> Result<PathBuf, ProjectError> {
    let stem = Path::new(level_file_name).file_stem().and_then(|stem| stem.to_str()).unwrap_or("Level");
    Ok(project.directory(project_file, "Saved/Autosaves")?.join(format!("{stem}.autosave.jarviglevel")))
}

fn parse_level_text(text: &str) -> Result<LevelDocument, crate::level::LevelError> {
    parse_level(text)
}

fn reject_absolute(path: &str) -> Result<(), ProjectError> {
    if path.is_empty() || path.contains(':') || path.starts_with('/') || path.starts_with('\\') || path.split(['/', '\\']).any(|part| part == "..") {
        return Err(ProjectError::AbsolutePath(format!("path {path} is not project-relative")));
    }
    Ok(())
}

fn required_str<'a>(json: &'a Json, key: &str) -> Result<&'a str, ProjectError> {
    json.get(key).and_then(Json::as_str).ok_or_else(|| ProjectError::Corrupt(format!("{key} is missing")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lighting_lab_level, ComponentRecord};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch() -> PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("jarvig-level-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn atomic_save_keeps_the_previous_level_and_a_backup() {
        let root = scratch();
        let project = ProjectDocument::lighting_lab();
        let project_file = root.join("LightingLab.jarvigproject");
        create_project_directories(&project_file, &project).unwrap();
        save_project_atomic(&project_file, &root.join("Saved/Backup"), &project).unwrap();
        let level_path = project.startup_level_path(&project_file).unwrap();
        let backup = root.join("Saved/Backup");
        let original = lighting_lab_level();
        save_level_atomic(&level_path, &backup, &original).unwrap();
        let mut moved = original.clone();
        let cube = moved.entities.iter_mut().find(|entity| entity.name == "White Cube").unwrap();
        if let ComponentRecord::Transform { translation, .. } = &mut cube.components[0] {
            translation.x = -1.0;
        } else {
            panic!("cube transform missing");
        }
        save_level_atomic(&level_path, &backup, &moved).unwrap();
        let loaded = load_level_file(&level_path).unwrap();
        let cube = loaded.entities.iter().find(|entity| entity.name == "White Cube").unwrap();
        match &cube.components[0] {
            ComponentRecord::Transform { translation, .. } => assert!((translation.x + 1.0).abs() < 1.0e-9),
            _ => panic!("transform missing"),
        }
        let backed = load_level_file(&backup.join("LightingLab.jarviglevel")).unwrap();
        let backed_cube = backed.entities.iter().find(|entity| entity.name == "White Cube").unwrap();
        match &backed_cube.components[0] {
            ComponentRecord::Transform { translation, .. } => assert!((translation.x + 1.55).abs() < 1.0e-9),
            _ => panic!("backup transform missing"),
        }
        let mut bad = moved.clone();
        bad.format_version = 9;
        let before = fs::read(&level_path).unwrap();
        assert!(save_level_atomic(&level_path, &backup, &bad).is_err());
        assert_eq!(fs::read(&level_path).unwrap(), before);
        let autosave = autosave_path(&project_file, &project, "LightingLab.jarviglevel").unwrap();
        assert!(autosave.ends_with("LightingLab.autosave.jarviglevel"));
        assert_ne!(autosave, level_path);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn the_committed_lighting_lab_matches_the_document() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/lighting-lab");
        let project = load_project_file(&root.join("LightingLab.jarvigproject")).expect("sample project");
        assert_eq!(project.display_name, "Lighting Lab");
        assert_eq!(project.startup_level, "Content/Levels/LightingLab.jarviglevel");
        let level = load_level_file(&project.startup_level_path(&root.join("LightingLab.jarvigproject")).unwrap()).unwrap();
        assert_eq!(level.entities.len(), 16);
        let again = parse_level(&level.to_json()).expect("rewritten level");
        assert_eq!(again.to_json(), level.to_json());
        let floor = level.entities.iter().find(|entity| entity.name == "Floor").expect("floor");
        let floor_material = floor.components.iter().find_map(|component| match component {
            crate::ComponentRecord::MeshRenderer { material, .. } => Some(material),
            _ => None,
        }).expect("floor material");
        assert_eq!(floor_material.scheme, crate::MaterialScheme::Staged);
        assert_eq!(floor_material.name, "Tiles101");
        assert!((floor_material.uv_scale - 1.0).abs() < 1.0e-6);
        assert_eq!(floor_material.normal_convention, Some(crate::NormalConvention::DirectXNegativeY));
        for (name, scale) in [("Tile Plane", 0.5_f32), ("Tile Cube", 2.0), ("Tile Sphere", 4.0)] {
            let entity = level.entities.iter().find(|entity| entity.name == name).unwrap_or_else(|| panic!("{name}"));
            let material = entity.components.iter().find_map(|component| match component {
                crate::ComponentRecord::MeshRenderer { material, .. } => Some(material),
                _ => None,
            }).unwrap_or_else(|| panic!("{name} material"));
            assert_eq!(material.name, "Tiles101");
            assert!((material.uv_scale - scale).abs() < 1.0e-6, "{name} uv {}", material.uv_scale);
            assert!((material.roughness - 1.0).abs() < 1.0e-6);
            assert!((material.normal_scale - 1.0).abs() < 1.0e-6);
        }
        let gravel = level.entities.iter().find(|entity| entity.name == "Gravel Plane").expect("gravel");
        let gravel_material = gravel.components.iter().find_map(|component| match component {
            crate::ComponentRecord::MeshRenderer { material, .. } => Some(material),
            _ => None,
        }).expect("gravel material");
        assert_eq!(gravel_material.name, "Gravel035");
        assert!(!level.to_json().contains("PNG"));
        let white = level.entities.iter().find(|entity| entity.name == "White Cube").expect("white cube");
        match &white.components[0] {
            ComponentRecord::Transform { translation, .. } => assert!(translation.x < -2.0),
            _ => panic!("white cube transform missing"),
        }
        let near = level.entities.iter().find(|entity| entity.name == "Near Triangle").expect("near");
        match &near.components[0] {
            ComponentRecord::Transform { translation, .. } => assert!(translation.z.is_finite()),
            _ => panic!("near transform missing"),
        }
        let directional = level.entities.iter().find(|entity| entity.name == "Directional Light").expect("directional");
        let light = directional.components.iter().find_map(|component| match component {
            ComponentRecord::DirectionalLight(light) => Some(light),
            _ => None,
        }).expect("directional payload");
        assert!(light.shadow.cast);
        assert!((light.shadow.depth_bias_m - 0.002).abs() < 1.0e-6);
        assert_eq!(light.shadow.cascade_count, 0);
        assert!((light.shadow.cascade_distribution - 0.5).abs() < 1.0e-6);
        let json = level.to_json();
        let world = level.instantiate().unwrap();
        let revision = world.revision();
        let before = world.extract(crate::RenderFrameId(1)).unwrap();
        let labels = |name: &str| -> Vec<&str> {
            let id = world.entity_outline().into_iter().find(|row| row.name == name).unwrap().uuid;
            world.component_stack(id).unwrap().into_iter().map(|item| item.role.label()).collect()
        };
        assert_eq!(labels("Floor"), vec!["Transform", "Mesh Renderer"]);
        assert_eq!(labels("Blue Point Light"), vec!["Transform", "Point Light"]);
        assert_eq!(labels("Warm Spot Light"), vec!["Transform", "Spot Light"]);
        assert_eq!(labels("Reflection Probe"), vec!["Transform", "Reflection Probe"]);
        assert_eq!(labels("World Settings"), vec!["World Settings"]);
        assert_eq!(world.entity_outline().len(), level.entities.len());
        for row in world.entity_outline() {
            assert!(world.component_stack(row.uuid).unwrap().iter().all(|item| item.role != crate::ComponentRole::Camera));
        }
        assert_eq!(world.revision(), revision);
        let after = world.extract(crate::RenderFrameId(2)).unwrap();
        assert_eq!(after.instance_count(), before.instance_count());
        assert_eq!(after.light_count(), before.light_count());
        assert_eq!(after.world_revision, before.world_revision);
        assert_eq!(level.to_json(), json);
    }

    #[test]
    fn absolute_project_paths_are_rejected() {
        let mut project = ProjectDocument::lighting_lab();
        project.startup_level = "C:/Game/Level.jarviglevel".into();
        assert!(project.validate().is_err());
        project = ProjectDocument::lighting_lab();
        project.content_directory = "../outside".into();
        assert!(project.validate().is_err());
    }
}

fn required_u32(json: &Json, key: &str) -> Result<u32, ProjectError> {
    let value = json.get(key).and_then(Json::as_f64).ok_or_else(|| ProjectError::Corrupt(format!("{key} is missing")))?;
    if value.fract() != 0.0 || value < 0.0 {
        return Err(ProjectError::Corrupt(format!("{key} is not a version")));
    }
    Ok(value as u32)
}
