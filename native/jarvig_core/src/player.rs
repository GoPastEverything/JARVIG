//! A player definition names one character asset. It is not the body and not a spawn.
//!
//! Schema `jarvig.player` version 1 stores the name and a project-relative character
//! path. An empty character path means none has been chosen yet. Controller, input,
//! camera, and movement stay out until a later version.
//! A level places a player with a Player Start component. The character file stays
//! in the content browser until something asks to preview or instantiate it.

use crate::json_lite::{parse_json, Json};
use crate::{EntityId, Quat, Vec3};

pub const PLAYER_SCHEMA: &str = "jarvig.player";
pub const PLAYER_FORMAT_VERSION: u32 = 1;

/// Gameplay configuration that uses one character asset. Not a mesh and not a pawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerDocument {
    pub format_version: u32,
    pub uuid: EntityId,
    pub name: String,
    /// Project-relative `*.jarvigcharacter` path. Empty means no character has been chosen.
    /// Not an absolute path.
    pub character: String,
}

/// Where a level places the player definition. The body is not this component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerStartRecord {
    /// Project-relative `*.jarvigplayer` path. Empty until a definition is chosen.
    pub player: String,
    /// Editor placement ghost. The level file may store the flag. The ghost entities are not saved.
    pub preview: bool,
}

impl PlayerDocument {
    pub fn validate(&self) -> Result<(), String> {
        if self.format_version != PLAYER_FORMAT_VERSION {
            return Err(format!("player format version {} is not {PLAYER_FORMAT_VERSION}", self.format_version));
        }
        if self.name.is_empty() || !self.uuid.is_persistent() {
            return Err("player name or uuid is invalid".into());
        }
        if self.character.is_empty() {
            return Ok(());
        }
        validate_relative_asset(&self.character, "jarvigcharacter")?;
        Ok(())
    }

    pub fn to_json(&self) -> String {
        Json::object(vec![
            ("schema", Json::string(PLAYER_SCHEMA)),
            ("format_version", Json::int(self.format_version as i64)),
            ("uuid", Json::string(self.uuid.to_string())),
            ("name", Json::string(&self.name)),
            ("character", Json::string(&self.character)),
        ])
        .write()
    }
}

impl PlayerStartRecord {
    pub fn validate(&self) -> Result<(), String> {
        if self.player.is_empty() {
            return Ok(());
        }
        validate_relative_asset(&self.player, "jarvigplayer")
    }
}

pub fn parse_player(text: &str) -> Result<PlayerDocument, String> {
    let json = parse_json(text).map_err(|error| error.0)?;
    let schema = json.get("schema").and_then(Json::as_str).ok_or("player schema is missing")?;
    if schema != PLAYER_SCHEMA {
        return Err(format!("schema {schema} is not {PLAYER_SCHEMA}"));
    }
    let format_version = json.get("format_version").and_then(Json::as_f64).ok_or("player format version is missing")?;
    if format_version.fract() != 0.0 || format_version <= 0.0 {
        return Err("player format version is not an integer".into());
    }
    let uuid = EntityId::parse(json.get("uuid").and_then(Json::as_str).unwrap_or("")).ok_or("player uuid is invalid")?;
    let name = json.get("name").and_then(Json::as_str).unwrap_or("").to_string();
    let character = json.get("character").and_then(Json::as_str).unwrap_or("").to_string();
    let document = PlayerDocument { format_version: format_version as u32, uuid, name, character };
    document.validate()?;
    Ok(document)
}

pub fn parse_player_start_fields(player: &str, preview: bool) -> Result<PlayerStartRecord, String> {
    let record = PlayerStartRecord { player: player.to_string(), preview };
    record.validate()?;
    Ok(record)
}

/// A content path the project can store. Forward slashes, no drive, no parent escape.
pub fn validate_relative_asset(path: &str, extension: &str) -> Result<(), String> {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains(':') {
        return Err(format!("{path} is not a project-relative {extension} path"));
    }
    if path.split('/').any(|part| part.is_empty() || part == "." || part == "..") {
        return Err(format!("{path} leaves the project"));
    }
    let expected = format!(".{extension}");
    if !path.ends_with(&expected) {
        return Err(format!("{path} is not a {extension} asset"));
    }
    Ok(())
}

/// Editor spawn marker in the entity's local frame. Forward is -Z. Not a mesh.
pub fn player_start_lines() -> Vec<(Vec3, Vec3)> {
    let mut lines = Vec::new();
    let v = Vec3::new;
    let foot = 0.16;
    lines.push((v(-foot, 0.02, foot * 0.6), v(foot, 0.02, foot * 0.6)));
    lines.push((v(foot, 0.02, foot * 0.6), v(0.0, 0.02, -foot)));
    lines.push((v(0.0, 0.02, -foot), v(-foot, 0.02, foot * 0.6)));
    lines.push((v(0.0, 0.02, 0.0), v(0.0, 1.35, 0.0)));
    lines.push((v(-0.22, 1.15, 0.0), v(0.22, 1.15, 0.0)));
    let head = v(0.0, 1.62, 0.0);
    let radius = 0.11;
    let steps = 8;
    for step in 0..steps {
        let a0 = step as f64 / steps as f64 * std::f64::consts::TAU;
        let a1 = (step + 1) as f64 / steps as f64 * std::f64::consts::TAU;
        lines.push((v(head.x + radius * a0.cos(), head.y + radius * a0.sin(), head.z), v(head.x + radius * a1.cos(), head.y + radius * a1.sin(), head.z)));
    }
    lines.push((v(0.0, 1.2, 0.0), v(0.0, 1.2, -0.48)));
    lines.push((v(0.0, 1.2, -0.48), v(0.09, 1.28, -0.32)));
    lines.push((v(0.0, 1.2, -0.48), v(-0.09, 1.28, -0.32)));
    lines.push((v(0.0, 1.2, -0.48), v(0.0, 1.32, -0.32)));
    lines
}

/// Applies a player-start field. Unknown fields are refused by the caller.
pub fn apply_player_start_property(record: &PlayerStartRecord, field: crate::FieldId, value: crate::PropertyValue) -> Result<PlayerStartRecord, crate::AuthoringError> {
    let mut next = record.clone();
    if field == crate::FIELD_PLAYER_DEFINITION {
        let crate::PropertyValue::String(path) = value else { return Err(crate::AuthoringError::WrongType) };
        next.player = path;
    } else if field == crate::FIELD_PREVIEW_CHARACTER {
        let crate::PropertyValue::Bool(preview) = value else { return Err(crate::AuthoringError::WrongType) };
        next.preview = preview;
    } else {
        return Err(crate::AuthoringError::FieldNotFound);
    }
    next.validate().map_err(|_| crate::AuthoringError::InvalidValue)?;
    Ok(next)
}

/// Spawn facing applied on top of the character root's bind rotation.
pub fn compose_spawn_rotation(spawn: Quat, bind: Quat) -> Quat {
    spawn.mul(bind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{empty_world_level, ComponentRecord, EntityId, EntityRecord, LevelDocument, LevelError, Quat, Vec3, LEVEL_FORMAT_VERSION, LEVEL_PLAYER_START_VERSION, LEVEL_TERRAIN_VERSION};

    fn player() -> PlayerDocument {
        PlayerDocument {
            format_version: PLAYER_FORMAT_VERSION,
            uuid: EntityId::parse("33333333-3333-4333-8333-3333333333e0").unwrap(),
            name: "DefaultPlayer".into(),
            character: "Content/Characters/Base Male.jarvigcharacter".into(),
        }
    }

    #[test]
    fn player_definition_round_trips_and_rejects_a_path_outside_the_project() {
        let document = player();
        let parsed = parse_player(&document.to_json()).unwrap();
        assert_eq!(parsed, document);
        assert!(parse_player(&document.to_json().replace("Base Male.jarvigcharacter", "Base Male.fbx")).is_err());
        let mut unchosen = document.clone();
        unchosen.character.clear();
        assert!(unchosen.validate().is_ok());
        assert_eq!(parse_player(&unchosen.to_json()).unwrap().character, "");
        let mut absolute = document.clone();
        absolute.character = "C:/Characters/Base.jarvigcharacter".into();
        assert!(absolute.validate().is_err());
        let mut escaped = document.clone();
        escaped.character = "Content/../Base.jarvigcharacter".into();
        assert!(escaped.validate().is_err());
        assert!(document.to_json().contains("jarvig.player"));
        assert!(!document.to_json().contains("speed"));
    }

    #[test]
    fn player_start_writes_level_version_5_and_older_levels_still_load() {
        let older = empty_world_level();
        assert_eq!(older.format_version, LEVEL_FORMAT_VERSION);
        assert!(crate::parse_level(&older.to_json()).is_ok());
        let mut document = older.clone();
        document.format_version = LEVEL_PLAYER_START_VERSION;
        document.entities.push(EntityRecord {
            uuid: EntityId::parse("33333333-3333-4333-8333-3333333333e1").unwrap(),
            name: "Player Start".into(),
            parent_uuid: None,
            components: vec![
                ComponentRecord::Transform { translation: Vec3::new(1.0, 0.0, 2.0), rotation: Quat::IDENTITY, scale: Vec3::new(1.0, 1.0, 1.0) },
                ComponentRecord::PlayerStart(PlayerStartRecord { player: "Content/Players/DefaultPlayer.jarvigplayer".into(), preview: true }),
            ],
        });
        let text = document.to_json();
        let parsed = crate::parse_level(&text).unwrap();
        assert_eq!(parsed.format_version, LEVEL_PLAYER_START_VERSION);
        let world = parsed.instantiate().unwrap();
        assert_eq!(world.entity_outline().len(), 2);
        assert_eq!(world.object_count(), 0);
        let outline = world.entity_outline();
        let start = outline.iter().find(|row| row.name == "Player Start").unwrap();
        assert_eq!(start.class, crate::AuthoringClass::PlayerStart);
        let record = world.authored_player_start(start.uuid).unwrap();
        assert!(record.preview);
        assert_eq!(record.player, "Content/Players/DefaultPlayer.jarvigplayer");
        let captured = LevelDocument::capture(&world, parsed.level_uuid, parsed.name.clone()).unwrap();
        assert_eq!(captured.format_version, LEVEL_PLAYER_START_VERSION);
        assert_eq!(crate::parse_level(&captured.to_json()).unwrap(), captured);
        let mut too_old = document.clone();
        too_old.format_version = LEVEL_TERRAIN_VERSION;
        assert!(matches!(too_old.validate(), Err(LevelError::Corrupt(_))));
        let mut future = document.clone();
        future.format_version = crate::LEVEL_BLOCK_VERSION + 1;
        assert!(matches!(future.validate(), Err(LevelError::UnsupportedVersion(7))));
        let lines = player_start_lines();
        assert!(lines.iter().any(|(from, to)| to.z < from.z));
    }

    #[test]
    fn a_derived_preview_is_absent_from_the_outline_and_the_saved_level() {
        let mut document = empty_world_level();
        document.format_version = LEVEL_PLAYER_START_VERSION;
        document.entities.push(EntityRecord {
            uuid: EntityId::parse("33333333-3333-4333-8333-3333333333e1").unwrap(),
            name: "Player Start".into(),
            parent_uuid: None,
            components: vec![
                ComponentRecord::Transform { translation: Vec3::new(0.0, 0.0, 0.0), rotation: Quat::IDENTITY, scale: Vec3::new(1.0, 1.0, 1.0) },
                ComponentRecord::PlayerStart(PlayerStartRecord { player: "Content/Players/DefaultPlayer.jarvigplayer".into(), preview: false }),
            ],
        });
        let mut world = document.instantiate().unwrap();
        let ghost = world.create_entity("Ghost Body");
        let ghost_id = world.resolve(ghost).unwrap();
        world.mark_derived(ghost_id).unwrap();
        assert!(world.entity_outline().iter().all(|row| row.name != "Ghost Body"));
        let captured = LevelDocument::capture(&world, document.level_uuid, document.name.clone()).unwrap();
        assert!(captured.entities.iter().all(|entity| entity.name != "Ghost Body"));
        assert_eq!(captured.entities.len(), 2);
        world.retire_derived(ghost_id).unwrap();
        assert!(world.entity_outline().iter().all(|row| row.name != "Ghost Body"));
    }

    /// Local sample. Does not import meshes and does not rewrite the level.
    #[test]
    #[ignore = "reads the local Base Characters level and player definition and does not import meshes"]
    fn base_characters_level_is_one_player_start() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/base-characters");
        let level_text = std::fs::read_to_string(root.join("Content/Levels/Base.jarviglevel")).expect("base level");
        let level = crate::parse_level(&level_text).expect("parse level");
        assert_eq!(level.format_version, LEVEL_PLAYER_START_VERSION);
        assert_eq!(level.name, "Base Characters");
        assert_eq!(level.entities.len(), 2);
        assert!(level.entities.iter().any(|entity| entity.name == "World Settings"));
        let start = level.entities.iter().find(|entity| entity.name == "Player Start").expect("player start");
        assert!(matches!(
            start.components.as_slice(),
            [
                ComponentRecord::Transform { .. },
                ComponentRecord::PlayerStart(PlayerStartRecord { player, preview: false })
            ] if player == "Content/Players/DefaultPlayer.jarvigplayer"
        ));
        assert!(!level_text.contains("\"Joint\""));
        assert!(!level_text.contains("upperArmMesh.002"));
        assert!(!level_text.to_ascii_lowercase().contains(".fbx"));
        let player_text = std::fs::read_to_string(root.join("Content/Players/DefaultPlayer.jarvigplayer")).expect("player");
        let player = parse_player(&player_text).expect("parse player");
        assert_eq!(player.name, "DefaultPlayer");
        assert_eq!(player.character, "Content/Characters/Base Male.jarvigcharacter");
        let male = std::fs::read_to_string(root.join("Content/Characters/Base Male.jarvigcharacter")).expect("male");
        let base = std::fs::read_to_string(root.join("Content/Characters/Base.jarvigcharacter")).expect("base");
        assert!(male.contains("\"schema\": \"jarvig.character\""));
        assert!(base.contains("\"schema\": \"jarvig.character\""));
        assert!(male.contains("upperArmMesh.002"));
        assert!(!male.contains("foreArmMesh.002"));
    }
}
