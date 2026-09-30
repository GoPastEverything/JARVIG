//! Standalone game host library. No editor dock, inspector, outliner, or gizmos.
//!
//! The authored level is loaded as source data. [`GameApplication`] instantiates the runtime world.

mod materials;

use std::fs;
use std::path::{Path, PathBuf};

use jarvig_core::{load_level_file, load_project_file, GameApplication, LevelDocument};
use jarvig_engine::EngineSession;

pub use materials::{find_material_dir, staged_names};

/// One running game. The engine world is the loaded source. `app` is the execution copy.
pub struct GameSession {
    pub engine: EngineSession,
    pub app: GameApplication,
    pub play: jarvig_core::PlayControl,
    pub project_name: String,
    pub level_name: String,
    pub project_file: PathBuf,
}

pub fn open_server(project_file: &Path) -> Result<GameSession, String> {
    open(project_file, false)
}

pub fn open_client(project_file: &Path) -> Result<GameSession, String> {
    open(project_file, true)
}

fn open(project_file: &Path, presents: bool) -> Result<GameSession, String> {
    let project = load_project_file(project_file).map_err(|error| error.to_string())?;
    let level_path = project.startup_level_path(project_file).map_err(|error| error.to_string())?;
    let document = load_level_file(&level_path).map_err(|error| error.to_string())?;
    let level_name = document.name.clone();
    let mut engine = if presents {
        EngineSession::editor().map_err(|error| format!("{error:?}"))?
    } else {
        EngineSession::server().map_err(|error| format!("{error:?}"))?
    };
    if presents {
        materials::install_staged_materials(&mut engine, project_file, &document)?;
    }
    let root = project_file.parent().ok_or("project file has no directory")?;
    let assets = jarvig_core::load_project_mesh_assets(root, &project.content_directory, &project.intermediate_directory)?;
    engine.replace_mesh_assets(assets.clone());
    engine.load_level(&document)?;
    let mut app = GameApplication::new();
    app.set_mesh_assets(assets);
    app.load(document).map_err(|error| error.to_string())?;
    app.start().map_err(|error| error.to_string())?;
    if presents {
        app.runtime_world_mut().map_err(|error| error.to_string())?.alias_draw_keys_from(engine.world());
    }
    let settings = jarvig_core::load_project_game_settings(project_file, &project)?;
    let play = {
        let world = app.runtime_world_mut().map_err(|error| error.to_string())?;
        jarvig_core::PlayControl::attach(world, &settings)?
    };
    if let Some(pawn) = play.pawn_camera() {
        app.possess_camera(pawn).map_err(|error| error.to_string())?;
    }
    Ok(GameSession {
        project_name: project.display_name,
        level_name,
        engine,
        app,
        play,
        project_file: project_file.to_path_buf(),
    })
}

/// Loose development output. No pak and no cook.
///
/// `Build/Windows-x64/Game.exe`, the project file, and `Content/`.
pub fn stage_development(project_file: &Path, host_exe: &Path) -> Result<PathBuf, String> {
    if !host_exe.is_file() {
        return Err(format!("game host is missing: {}", host_exe.display()));
    }
    let project = load_project_file(project_file).map_err(|error| error.to_string())?;
    let root = project_file.parent().ok_or("project file has no directory")?;
    let out = root.join("Build").join("Windows-x64");
    fs::create_dir_all(&out).map_err(|error| error.to_string())?;
    fs::copy(host_exe, out.join("Game.exe")).map_err(|error| error.to_string())?;
    let file_name = project_file.file_name().ok_or("project file has no name")?;
    fs::copy(project_file, out.join(file_name)).map_err(|error| error.to_string())?;
    if let Ok(settings) = project.directory(project_file, &project.settings) {
        if settings.is_file() {
            let dest = out.join(&project.settings);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::copy(&settings, &dest).map_err(|error| error.to_string())?;
        }
    }
    copy_dir(&root.join(&project.content_directory), &out.join(&project.content_directory))?;
    let level_path = project.startup_level_path(project_file).map_err(|error| error.to_string())?;
    if level_path.is_file() {
        let document = load_level_file(&level_path).map_err(|error| error.to_string())?;
        copy_staged_materials(project_file, &out, &document)?;
    }
    Ok(out)
}

fn copy_staged_materials(project_file: &Path, out: &Path, document: &LevelDocument) -> Result<(), String> {
    for name in staged_names(document) {
        let Some(source) = find_material_dir(project_file, &name) else {
            return Err(format!("staged material {name} was not found"));
        };
        let dest = out.join("Content").join("Materials").join(format!("{name}_4K-PNG"));
        if dest.is_dir() {
            continue;
        }
        copy_dir(&source, &dest)?;
    }
    Ok(())
}

fn copy_dir(source: &Path, dest: &Path) -> Result<(), String> {
    if !source.exists() {
        fs::create_dir_all(dest).map_err(|error| error.to_string())?;
        return Ok(());
    }
    fs::create_dir_all(dest).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if entry.file_type().map_err(|error| error.to_string())?.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::copy(&from, &to).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub fn project_beside_exe(exe: &Path) -> Result<PathBuf, String> {
    let dir = exe.parent().ok_or("game executable has no directory")?;
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("jarvigproject") {
            found.push(path);
        }
    }
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => Err("no .jarvigproject is next to the game executable".into()),
        _ => Err("more than one .jarvigproject is next to the game executable".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jarvig_core::EntityId;

    fn empty_level() -> String {
        r#"{
            "schema": "jarvig.level",
            "format_version": 1,
            "level_uuid": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            "name": "Empty",
            "world_settings": {
                "entity": "11111111-1111-4111-8111-111111111111",
                "enabled": true,
                "intensity": 0.2,
                "upper": [0.55, 0.68, 0.86],
                "lower": [0.22, 0.16, 0.11],
                "probe_update_policy": "static"
            },
            "entities": [{
                "uuid": "11111111-1111-4111-8111-111111111111",
                "name": "World Settings",
                "parent_uuid": null,
                "components": [{ "type": "WorldSettings", "version": 1 }]
            }]
        }"#
        .into()
    }

    #[test]
    fn the_game_crate_does_not_depend_on_the_editor() {
        let manifest = include_str!("../Cargo.toml");
        assert!(!manifest.contains("jarvig_editor"));
    }

    #[test]
    fn stage_writes_a_loose_game_tree_and_not_the_editor() {
        let root = std::env::temp_dir().join(format!("jarvig-stage-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("Content/Levels")).unwrap();
        fs::create_dir_all(root.join("Saved/Editor")).unwrap();
        fs::write(root.join("Saved/Editor/viewport.json"), "nope").unwrap();
        fs::write(root.join("Content/Levels/Empty.jarviglevel"), empty_level()).unwrap();
        let mut project = jarvig_core::ProjectDocument::lighting_lab();
        project.display_name = "Stage Test".into();
        project.project_uuid = EntityId::parse("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap();
        project.startup_level = "Content/Levels/Empty.jarviglevel".into();
        let project_file = root.join("StageTest.jarvigproject");
        fs::write(&project_file, project.to_json()).unwrap();
        let host = root.join("host.exe");
        fs::write(&host, b"not-a-real-exe").unwrap();
        let out = stage_development(&project_file, &host).unwrap();
        assert!(out.join("Game.exe").is_file());
        assert!(out.join("StageTest.jarvigproject").is_file());
        assert!(out.join("Content/Levels/Empty.jarviglevel").is_file());
        assert!(!out.join("Saved").exists());
        assert_eq!(fs::read(out.join("Game.exe")).unwrap(), b"not-a-real-exe");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_server_host_runs_the_lighting_lab_without_a_renderer() {
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../samples/lighting-lab/LightingLab.jarvigproject");
        let mut session = open_server(&project).unwrap();
        let revision = session.engine.world().revision();
        let count = session.engine.world().entity_count();
        assert!(count > 1);
        assert_eq!(session.app.runtime_world().unwrap().entity_count(), count + 1);
        assert!(session.play.pawn().is_some());
        assert!(session.engine.world().entity_outline().iter().all(|row| row.name != "Player"));
        session.app.runtime_world_mut().unwrap().create_entity("Runtime Only");
        assert_eq!(session.engine.world().revision(), revision);
        assert!(session.engine.world().entity_outline().iter().all(|row| row.name != "Runtime Only"));
        assert!(session.app.runtime_world().unwrap().entity_outline().iter().any(|row| row.name == "Runtime Only"));
        session.app.tick(1.0 / 60.0).unwrap();
        session.app.stop().unwrap();
        assert_eq!(session.engine.extraction_count(), 0);
        assert_eq!(session.engine.material_compile_count(), 0);
        assert_eq!(session.engine.texture_count(), 0);
        assert!(session.app.runtime_world().is_none());
    }
}
