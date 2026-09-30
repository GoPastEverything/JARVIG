//! Project gameplay settings. Not the level, and not implied for every project.

use std::fs;
use std::path::Path;

use crate::json_lite::{parse_json, Json};
use crate::{EntityId, ProjectDocument};

pub const SETTINGS_SCHEMA: &str = "jarvig.settings";
pub const SETTINGS_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PawnSelection {
    None,
    DefaultFreeFly,
    Entity(EntityId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupCameraPolicy {
    /// The possessed pawn's Camera. Not the editor camera.
    Pawn,
    /// The level's authored `startup_camera`, if it named one.
    Authored,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameSettings {
    pub player_controller: bool,
    pub pawn: PawnSelection,
    pub mapping_context: Option<String>,
    pub startup_camera: StartupCameraPolicy,
}

impl GameSettings {
    pub fn inactive() -> Self {
        Self {
            player_controller: false,
            pawn: PawnSelection::None,
            mapping_context: None,
            startup_camera: StartupCameraPolicy::Authored,
        }
    }

    pub fn drives_a_pawn(&self) -> bool {
        self.player_controller && !matches!(self.pawn, PawnSelection::None) && self.mapping_context.is_some()
    }

    pub fn to_json(&self) -> String {
        let pawn = match self.pawn {
            PawnSelection::None => "none".to_string(),
            PawnSelection::DefaultFreeFly => "JARVIG.DefaultFreeFlyPawn".to_string(),
            PawnSelection::Entity(id) => id.to_string(),
        };
        let mapping = self.mapping_context.clone().unwrap_or_else(|| "none".into());
        Json::object(vec![
            ("schema", Json::string(SETTINGS_SCHEMA)),
            ("format_version", Json::int(SETTINGS_FORMAT_VERSION as i64)),
            ("default_player_controller", Json::string(if self.player_controller { "JARVIG.PlayerController" } else { "none" })),
            ("default_pawn", Json::string(pawn)),
            ("default_mapping_context", Json::string(mapping)),
            ("startup_camera", Json::string(if self.startup_camera == StartupCameraPolicy::Pawn { "pawn" } else { "authored" })),
        ])
        .write()
    }
}

/// Missing files and the old placeholder sentence mean no gameplay. A schema document that is wrong is an error.
pub fn load_game_settings(path: &Path) -> Result<GameSettings, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(_) => return Ok(GameSettings::inactive()),
    };
    let trimmed = text.trim();
    if !trimmed.starts_with('{') {
        return Ok(GameSettings::inactive());
    }
    let json = parse_json(trimmed).map_err(|error| error.to_string())?;
    let schema = json.get("schema").and_then(Json::as_str).unwrap_or("");
    if schema != SETTINGS_SCHEMA {
        return Err(format!("settings schema {schema} is not {SETTINGS_SCHEMA}"));
    }
    let version = json.get("format_version").and_then(Json::as_f64).unwrap_or(0.0) as u32;
    if version == 0 || version > SETTINGS_FORMAT_VERSION {
        return Err(format!("settings format version {version} is not supported"));
    }
    let controller = required_str(&json, "default_player_controller")?;
    let pawn = required_str(&json, "default_pawn")?;
    let mapping = required_str(&json, "default_mapping_context")?;
    let camera = required_str(&json, "startup_camera")?;
    let player_controller = match controller {
        "none" => false,
        "JARVIG.PlayerController" => true,
        other => return Err(format!("unknown player controller {other}")),
    };
    let pawn = match pawn {
        "none" => PawnSelection::None,
        "JARVIG.DefaultFreeFlyPawn" => PawnSelection::DefaultFreeFly,
        other => PawnSelection::Entity(EntityId::parse(other).ok_or_else(|| format!("default pawn {other} is not a known pawn or a uuid"))?),
    };
    let mapping_context = match mapping {
        "none" => None,
        "JARVIG.Default" => Some("JARVIG.Default".into()),
        other => return Err(format!("unknown mapping context {other}")),
    };
    let startup_camera = match camera {
        "pawn" => StartupCameraPolicy::Pawn,
        "authored" => StartupCameraPolicy::Authored,
        other => return Err(format!("unknown startup camera policy {other}")),
    };
    Ok(GameSettings { player_controller, pawn, mapping_context, startup_camera })
}

pub fn load_project_game_settings(project_file: &Path, project: &ProjectDocument) -> Result<GameSettings, String> {
    let path = project.directory(project_file, &project.settings).map_err(|error| error.to_string())?;
    load_game_settings(&path)
}

fn required_str<'a>(json: &'a Json, key: &str) -> Result<&'a str, String> {
    json.get(key).and_then(Json::as_str).ok_or_else(|| format!("settings {key} is missing"))
}
