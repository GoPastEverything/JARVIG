//! JARVIG.exe. The project launcher. It does not construct an editor or a world.
//!
//! A project path skips the window and starts JARVIGEditor. Harness flags stay on JARVIGEditor.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let project = match project_from_args(&args) {
        Ok(Some(path)) => Some(path),
        Ok(None) => match jarvig_hub::pick_project() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("JARVIG_FAIL {error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("JARVIG_FAIL {error}");
            std::process::exit(1);
        }
    };
    let Some(project) = project else {
        std::process::exit(0);
    };
    match spawn_editor(&project) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("JARVIG_FAIL {error}");
            std::process::exit(1);
        }
    }
}

fn project_from_args(args: &[String]) -> Result<Option<PathBuf>, String> {
    if let Some(index) = args.iter().position(|arg| arg == "--project") {
        let path = args.get(index + 1).ok_or("--project needs a .jarvigproject path")?;
        return Ok(Some(PathBuf::from(path)));
    }
    if let Some(path) = args.iter().find(|arg| arg.to_ascii_lowercase().ends_with(".jarvigproject")) {
        return Ok(Some(PathBuf::from(path)));
    }
    Ok(None)
}

fn spawn_editor(project: &Path) -> Result<i32, String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let editor = exe.parent().ok_or("JARVIG has no directory")?.join("JARVIGEditor.exe");
    if !editor.is_file() {
        return Err(format!(
            "JARVIGEditor.exe was not next to {}. Build the editor, or launch JARVIGEditor directly.",
            exe.display()
        ));
    }
    let status = Command::new(&editor).arg("--project").arg(project).status().map_err(|error| error.to_string())?;
    Ok(status.code().unwrap_or(1))
}
