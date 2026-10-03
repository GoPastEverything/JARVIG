//! Project hub data. Recent projects and the files a template writes.
//!
//! This module does not open a window, instantiate a world, scan assets, or
//! build meshlets. Hosts call it, then decide whether to start an editor.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::json_lite::{parse_json, Json};
use crate::level::{ComponentRecord, EntityRecord};
use crate::player::{PlayerDocument, PlayerStartRecord};
use crate::settings::GameSettings;
use crate::{
    create_project_directories, empty_world_level, load_project_file, save_atomic, save_level_atomic, save_project_atomic, EntityId, LevelDocument,
    ProjectDocument, LEVEL_PLAYER_START_VERSION, LIGHTING_LAB_PROJECT_UUID, PROJECT_FORMAT_VERSION, Quat, Vec3,
};

pub const HUB_RECENT_SCHEMA: &str = "jarvig.hub-recent";
pub const HUB_RECENT_VERSION: u32 = 1;
pub const TEMPLATE_STARTUP_LEVEL: &str = "Content/Levels/Main.jarviglevel";
pub const TEMPLATE_PLAYER_FILE: &str = "Content/Players/DefaultPlayer.jarvigplayer";
/// Shown when a First Person or Third Person project opens. Those templates do not invent a pawn.
pub const PLAYER_TEMPLATE_NOTE: &str =
    "This template places a Player Start and a player definition. A controller, a camera, and input are not in this foundation.";

const RECENT_CAP: usize = 16;

/// The four project templates this foundation writes. Later templates are new files, not editor branches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectTemplate {
    Blank,
    FirstPerson,
    ThirdPerson,
    Landscape,
}

impl ProjectTemplate {
    pub fn word(self) -> &'static str {
        match self {
            Self::Blank => "blank",
            Self::FirstPerson => "first-person",
            Self::ThirdPerson => "third-person",
            Self::Landscape => "landscape",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Blank => "Blank",
            Self::FirstPerson => "First Person",
            Self::ThirdPerson => "Third Person",
            Self::Landscape => "Landscape",
        }
    }

    pub fn summary(self) -> &'static str {
        match self {
            Self::Blank => "Empty world and basic World Settings.",
            Self::FirstPerson => "Player Start and a player definition. No controller yet.",
            Self::ThirdPerson => "Player Start and a player definition. No camera yet.",
            Self::Landscape => "Empty world. Land is available so you can create terrain.",
        }
    }

    /// Older menu words still name these templates. The file stores [`Self::word`].
    pub fn parse(word: &str) -> Option<Self> {
        match word.trim().to_ascii_lowercase().as_str() {
            "blank" | "empty" => Some(Self::Blank),
            "first-person" | "fps" => Some(Self::FirstPerson),
            "third-person" => Some(Self::ThirdPerson),
            "landscape" | "terrain" | "land" => Some(Self::Landscape),
            _ => None,
        }
    }

    pub fn opens_land(self) -> bool {
        matches!(self, Self::Landscape)
    }

    pub fn places_player(self) -> bool {
        matches!(self, Self::FirstPerson | Self::ThirdPerson)
    }

    pub fn all() -> [Self; 4] {
        [Self::Blank, Self::FirstPerson, Self::ThirdPerson, Self::Landscape]
    }
}

/// One lightweight recent-project row. A missing file stays in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentProject {
    pub path: PathBuf,
    pub name: String,
    pub engine_version: String,
    pub startup_level: String,
    pub last_opened_unix: i64,
    pub thumbnail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentList {
    pub projects: Vec<RecentProject>,
}

impl RecentList {
    pub fn is_missing(&self, index: usize) -> bool {
        self.projects.get(index).is_some_and(|entry| project_file_missing(&entry.path))
    }
}

/// The hub's own list. It is not inside a game project and it is not Lighting Lab.
pub fn hub_recent_file() -> PathBuf {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        if !local.is_empty() {
            return PathBuf::from(local).join("JARVIG").join("Hub").join("recent-projects.json");
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            return parent.join("Saved").join("Hub").join("recent-projects.json");
        }
    }
    PathBuf::from("Saved").join("Hub").join("recent-projects.json")
}

pub fn project_file_missing(path: &Path) -> bool {
    !path.is_file()
}

pub fn load_recent_at(path: &Path) -> Result<RecentList, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(RecentList { projects: Vec::new() }),
        Err(error) => return Err(error.to_string()),
    };
    parse_recent(&text)
}

pub fn save_recent_at(path: &Path, list: &RecentList) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let bytes = list.to_json().into_bytes();
    let backup = path.parent().unwrap_or(Path::new(".")).join("backup");
    save_atomic(path, &backup, &bytes).map_err(|error| error.to_string())
}

/// Read the project manifest and move that path to the front. A missing file is left where it was.
pub fn remember_recent_at(store: &Path, project_file: &Path) -> Result<RecentList, String> {
    if project_file_missing(project_file) {
        return Err(format!("{} is missing. It was not replaced.", project_file.display()));
    }
    let project = load_project_file(project_file).map_err(|error| error.to_string())?;
    let mut list = load_recent_at(store)?;
    let thumbnail = list
        .projects
        .iter()
        .find(|entry| same_project_path(&entry.path, project_file))
        .map(|entry| entry.thumbnail.clone())
        .unwrap_or_default();
    list.projects.retain(|entry| !same_project_path(&entry.path, project_file));
    list.projects.insert(
        0,
        RecentProject {
            path: project_file.to_path_buf(),
            name: project.display_name,
            engine_version: project.engine_version,
            startup_level: project.startup_level,
            last_opened_unix: unix_now(),
            thumbnail,
        },
    );
    list.projects.truncate(RECENT_CAP);
    save_recent_at(store, &list)?;
    Ok(list)
}

pub fn remove_recent_at(store: &Path, project_file: &Path) -> Result<RecentList, String> {
    let mut list = load_recent_at(store)?;
    list.projects.retain(|entry| !same_project_path(&entry.path, project_file));
    save_recent_at(store, &list)?;
    Ok(list)
}

/// Point an old row at a project file the user located. The old path is not replaced by a different recent row.
pub fn locate_recent_at(store: &Path, from: &Path, to: &Path) -> Result<RecentList, String> {
    if project_file_missing(to) {
        return Err(format!("{} is missing. It was not replaced.", to.display()));
    }
    let project = load_project_file(to).map_err(|error| error.to_string())?;
    let mut list = load_recent_at(store)?;
    let Some(entry) = list.projects.iter_mut().find(|entry| same_project_path(&entry.path, from)) else {
        return Err(format!("{} is not in the recent list.", from.display()));
    };
    entry.path = to.to_path_buf();
    entry.name = project.display_name;
    entry.engine_version = project.engine_version;
    entry.startup_level = project.startup_level;
    entry.last_opened_unix = unix_now();
    save_recent_at(store, &list)?;
    Ok(list)
}

pub fn same_project_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    normalize_path(left) == normalize_path(right)
}

/// Write an ordinary project. Templates are these files. They are not a mode hard-coded into the editor.
///
/// The destination project file and startup level must not already exist. Other files beside the
/// project root are left alone. A host that wants a fresh folder checks that folder first.
pub fn create_project_at(project_file: &Path, display_name: &str, template: ProjectTemplate) -> Result<ProjectDocument, String> {
    let display_name = display_name.trim();
    if display_name.is_empty() || display_name.contains(['\n', '\r']) {
        return Err("A project needs a name.".into());
    }
    if project_file.extension().and_then(|ext| ext.to_str()) != Some("jarvigproject") {
        return Err("A project file ends in .jarvigproject.".into());
    }
    if project_file.exists() {
        return Err(format!("{} already exists. A new project was not written.", project_file.display()));
    }
    let project = ProjectDocument {
        format_version: PROJECT_FORMAT_VERSION,
        project_uuid: EntityId::new(),
        display_name: display_name.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").into(),
        startup_level: TEMPLATE_STARTUP_LEVEL.into(),
        content_directory: "Content".into(),
        saved_directory: "Saved".into(),
        config_directory: "Config".into(),
        intermediate_directory: "Intermediate".into(),
        settings: "Config/Project.jarvigsettings".into(),
    };
    if project.project_uuid.to_string() == LIGHTING_LAB_PROJECT_UUID {
        return Err("A new project must not reuse the Lighting Lab identity.".into());
    }
    let root = project_file.parent().filter(|path| !path.as_os_str().is_empty()).ok_or("project file has no directory")?;
    let level_path = root.join(TEMPLATE_STARTUP_LEVEL);
    if level_path.exists() {
        return Err(format!("{} already exists. A new project was not written.", level_path.display()));
    }
    create_project_directories(project_file, &project).map_err(|error| error.to_string())?;
    let backup = root.join("Saved").join("Backup");
    let level = startup_level(display_name, template);
    save_level_atomic(&level_path, &backup, &level).map_err(|error| error.to_string())?;
    if template.places_player() {
        let player_path = root.join(TEMPLATE_PLAYER_FILE);
        if player_path.exists() {
            return Err(format!("{} already exists. A new project was not written.", player_path.display()));
        }
        if let Some(parent) = player_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let player = PlayerDocument {
            format_version: crate::PLAYER_FORMAT_VERSION,
            uuid: EntityId::new(),
            name: "DefaultPlayer".into(),
            character: String::new(),
        };
        player.validate()?;
        save_atomic(&player_path, &backup, player.to_json().as_bytes()).map_err(|error| error.to_string())?;
    }
    let settings = GameSettings::inactive();
    let settings_path = root.join(&project.settings);
    save_atomic(&settings_path, &backup, settings.to_json().as_bytes()).map_err(|error| error.to_string())?;
    let template_path = root.join("Saved").join("Editor").join("template.txt");
    save_atomic(&template_path, &backup, format!("{}\n", template.word()).as_bytes()).map_err(|error| error.to_string())?;
    save_project_atomic(project_file, &backup, &project).map_err(|error| error.to_string())?;
    Ok(project)
}

fn startup_level(display_name: &str, template: ProjectTemplate) -> LevelDocument {
    let mut level = empty_world_level();
    level.name = display_name.to_string();
    if template.places_player() {
        level.format_version = LEVEL_PLAYER_START_VERSION;
        level.entities.push(EntityRecord {
            uuid: EntityId::new(),
            name: "Player Start".into(),
            parent_uuid: None,
            components: vec![
                ComponentRecord::Transform { translation: Vec3::new(0.0, 0.0, 0.0), rotation: Quat::IDENTITY, scale: Vec3::new(1.0, 1.0, 1.0) },
                ComponentRecord::PlayerStart(PlayerStartRecord { player: TEMPLATE_PLAYER_FILE.into(), preview: false }),
            ],
        });
    }
    level
}

impl RecentList {
    fn to_json(&self) -> String {
        let projects = Json::array(self.projects.iter().map(RecentProject::to_json).collect());
        Json::object(vec![
            ("schema", Json::string(HUB_RECENT_SCHEMA)),
            ("format_version", Json::int(HUB_RECENT_VERSION as i64)),
            ("projects", projects),
        ])
        .write()
    }
}

impl RecentProject {
    fn to_json(&self) -> Json {
        Json::object(vec![
            ("path", Json::string(self.path.to_string_lossy())),
            ("name", Json::string(&self.name)),
            ("engine_version", Json::string(&self.engine_version)),
            ("startup_level", Json::string(&self.startup_level)),
            ("last_opened_unix", Json::int(self.last_opened_unix)),
            ("thumbnail", Json::string(&self.thumbnail)),
        ])
    }
}

fn parse_recent(text: &str) -> Result<RecentList, String> {
    let json = parse_json(text).map_err(|error| error.to_string())?;
    let schema = json.get("schema").and_then(Json::as_str).ok_or("recent schema is missing")?;
    if schema != HUB_RECENT_SCHEMA {
        return Err(format!("schema {schema} is not {HUB_RECENT_SCHEMA}"));
    }
    let version = json.get("format_version").and_then(Json::as_f64).ok_or("recent format version is missing")?;
    if version.fract() != 0.0 || version < 1.0 || (version as u32) > HUB_RECENT_VERSION {
        return Err(format!("recent format version {version} is not supported"));
    }
    let projects = json.get("projects").and_then(Json::as_array).ok_or("recent projects are missing")?;
    let mut list = Vec::new();
    for item in projects {
        let path = item.get("path").and_then(Json::as_str).ok_or("a recent project has no path")?;
        if path.is_empty() {
            return Err("a recent project path is empty".into());
        }
        let name = item.get("name").and_then(Json::as_str).unwrap_or("").to_string();
        list.push(RecentProject {
            path: PathBuf::from(path),
            name: if name.is_empty() { Path::new(path).file_stem().and_then(|stem| stem.to_str()).unwrap_or("Project").to_string() } else { name },
            engine_version: item.get("engine_version").and_then(Json::as_str).unwrap_or("").to_string(),
            startup_level: item.get("startup_level").and_then(Json::as_str).unwrap_or("").to_string(),
            last_opened_unix: item.get("last_opened_unix").and_then(Json::as_f64).unwrap_or(0.0) as i64,
            thumbnail: item.get("thumbnail").and_then(Json::as_str).unwrap_or("").to_string(),
        });
    }
    list.truncate(RECENT_CAP);
    Ok(RecentList { projects: list })
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_ascii_lowercase()
}

fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_secs() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{load_game_settings, parse_level, parse_player, ComponentRecord, PawnSelection, LEVEL_FORMAT_VERSION, LEVEL_PLAYER_START_VERSION};

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("jarvig-hub-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn template_words_cover_the_old_menu_and_the_new_names() {
        assert_eq!(ProjectTemplate::parse("empty"), Some(ProjectTemplate::Blank));
        assert_eq!(ProjectTemplate::parse("blank"), Some(ProjectTemplate::Blank));
        assert_eq!(ProjectTemplate::parse("terrain"), Some(ProjectTemplate::Landscape));
        assert_eq!(ProjectTemplate::parse("land"), Some(ProjectTemplate::Landscape));
        assert_eq!(ProjectTemplate::parse("landscape"), Some(ProjectTemplate::Landscape));
        assert_eq!(ProjectTemplate::parse("fps"), Some(ProjectTemplate::FirstPerson));
        assert_eq!(ProjectTemplate::parse("first-person"), Some(ProjectTemplate::FirstPerson));
        assert_eq!(ProjectTemplate::parse("third-person"), Some(ProjectTemplate::ThirdPerson));
        assert!(ProjectTemplate::parse("quake").is_none());
        assert!(ProjectTemplate::Landscape.opens_land());
        assert!(!ProjectTemplate::Blank.opens_land());
        assert!(PLAYER_TEMPLATE_NOTE.contains("controller"));
        assert!(PLAYER_TEMPLATE_NOTE.contains("not in this foundation"));
    }

    #[test]
    fn blank_project_is_world_settings_only() {
        let root = scratch("blank");
        let project_file = root.join("Blank").join("Blank.jarvigproject");
        let project = create_project_at(&project_file, "Blank", ProjectTemplate::Blank).unwrap();
        assert_ne!(project.project_uuid.to_string(), LIGHTING_LAB_PROJECT_UUID);
        assert_eq!(project.startup_level, TEMPLATE_STARTUP_LEVEL);
        assert_eq!(project.display_name, "Blank");
        let level = parse_level(&fs::read_to_string(root.join("Blank").join(TEMPLATE_STARTUP_LEVEL)).unwrap()).unwrap();
        assert_eq!(level.format_version, LEVEL_FORMAT_VERSION);
        assert_eq!(level.entities.len(), 1);
        assert_eq!(level.entities[0].name, "World Settings");
        assert!(level.entities.iter().all(|entity| {
            entity.components.iter().all(|component| matches!(component, ComponentRecord::WorldSettings))
        }));
        let text = level.to_json();
        for forbidden in ["townshop", "Base Male", "Base.jarvigcharacter", "Near Triangle", "DirectionalLight", "Joint", "PlayerStart"] {
            assert!(!text.contains(forbidden), "{forbidden} leaked into a blank level");
        }
        let settings = load_game_settings(&root.join("Blank").join(&project.settings)).unwrap();
        assert!(!settings.player_controller);
        assert_eq!(settings.pawn, PawnSelection::None);
        assert!(!settings.to_json().contains("DefaultFreeFlyPawn"));
        assert_eq!(fs::read_to_string(root.join("Blank/Saved/Editor/template.txt")).unwrap().trim(), "blank");
        assert!(create_project_at(&project_file, "Blank", ProjectTemplate::Blank).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn first_and_third_person_place_a_player_start_without_a_body() {
        let root = scratch("players");
        for template in [ProjectTemplate::FirstPerson, ProjectTemplate::ThirdPerson] {
            let folder = root.join(template.word());
            let project_file = folder.join(format!("{}.jarvigproject", template.word()));
            create_project_at(&project_file, template.label(), template).unwrap();
            let level = parse_level(&fs::read_to_string(folder.join(TEMPLATE_STARTUP_LEVEL)).unwrap()).unwrap();
            assert_eq!(level.format_version, LEVEL_PLAYER_START_VERSION);
            assert_eq!(level.entities.len(), 2);
            assert!(level.entities.iter().any(|entity| entity.name == "World Settings"));
            let start = level.entities.iter().find(|entity| entity.name == "Player Start").unwrap();
            match start.components.iter().find(|component| matches!(component, ComponentRecord::PlayerStart(_))) {
                Some(ComponentRecord::PlayerStart(record)) => {
                    assert_eq!(record.player, TEMPLATE_PLAYER_FILE);
                    assert!(!record.preview);
                }
                _ => panic!("player start missing"),
            }
            let player = parse_player(&fs::read_to_string(folder.join(TEMPLATE_PLAYER_FILE)).unwrap()).unwrap();
            assert_eq!(player.character, "");
            assert!(!player.to_json().contains("Base"));
            assert_eq!(fs::read_to_string(folder.join("Saved/Editor/template.txt")).unwrap().trim(), template.word());
            let world = level.instantiate().unwrap();
            assert_eq!(world.object_count(), 0);
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn landscape_is_an_empty_world_with_the_land_word() {
        let root = scratch("land");
        let project_file = root.join("Ridge").join("Ridge.jarvigproject");
        create_project_at(&project_file, "Ridge", ProjectTemplate::Landscape).unwrap();
        let level = parse_level(&fs::read_to_string(root.join("Ridge").join(TEMPLATE_STARTUP_LEVEL)).unwrap()).unwrap();
        assert_eq!(level.entities.len(), 1);
        assert!(level.entities.iter().all(|entity| !entity.components.iter().any(|component| matches!(component, ComponentRecord::Terrain(_)))));
        assert_eq!(fs::read_to_string(root.join("Ridge/Saved/Editor/template.txt")).unwrap().trim(), "landscape");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn recent_list_keeps_a_missing_project_and_lives_outside_it() {
        let root = scratch("recent");
        let store = root.join("hub").join("recent-projects.json");
        let first = root.join("One").join("One.jarvigproject");
        let second = root.join("Two").join("Two.jarvigproject");
        create_project_at(&first, "One", ProjectTemplate::Blank).unwrap();
        create_project_at(&second, "Two", ProjectTemplate::Blank).unwrap();
        remember_recent_at(&store, &first).unwrap();
        let listed = remember_recent_at(&store, &second).unwrap();
        assert_eq!(listed.projects[0].path, second);
        assert!(same_project_path(&listed.projects[1].path, &first));
        fs::remove_file(&first).unwrap();
        let loaded = load_recent_at(&store).unwrap();
        assert!(project_file_missing(&loaded.projects[1].path));
        assert!(loaded.is_missing(1));
        assert!(remember_recent_at(&store, &first).is_err());
        let still = load_recent_at(&store).unwrap();
        assert_eq!(still.projects.len(), 2);
        assert!(still.projects.iter().any(|entry| same_project_path(&entry.path, &first)));
        let moved = root.join("Moved").join("Moved.jarvigproject");
        create_project_at(&moved, "Moved", ProjectTemplate::Blank).unwrap();
        let located = locate_recent_at(&store, &first, &moved).unwrap();
        assert!(located.projects.iter().any(|entry| same_project_path(&entry.path, &moved)));
        assert!(!located.projects.iter().any(|entry| same_project_path(&entry.path, &first)));
        let removed = remove_recent_at(&store, &moved).unwrap();
        assert!(!removed.projects.iter().any(|entry| same_project_path(&entry.path, &moved)));
        assert!(!store.starts_with(root.join("One")));
        assert!(!store.to_string_lossy().contains(".jarvigproject"));
        let home = hub_recent_file();
        assert_eq!(home.file_name().and_then(|name| name.to_str()), Some("recent-projects.json"));
        assert!(home.to_string_lossy().contains("Hub"));
        assert!(!home.to_string_lossy().to_ascii_lowercase().contains("lighting-lab"));
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            if !local.is_empty() {
                assert!(home.starts_with(local));
            }
        }
        let _ = fs::remove_dir_all(&root);
    }
}
