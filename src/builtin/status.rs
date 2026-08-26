use serde::Serialize;
use std::ffi::OsString;

use crate::activation::{self, LibraryState};
use crate::paths::AppPaths;
use crate::source;

use super::write_json;

#[derive(Serialize)]
struct StatusOutput {
    initialized: bool,
    current_activation: Option<String>,
    source: Option<LibraryState>,
    active: Option<LibraryState>,
    pending_changes: bool,
}

pub fn run(arguments: &[OsString], paths: &AppPaths) -> Result<i32, String> {
    let mut json = false;
    for argument in arguments {
        match argument.to_str() {
            Some("--json") if !json => json = true,
            Some(value) => return Err(format!("status: unknown or duplicate option '{value}'")),
            None => return Err("status: options must be valid UTF-8".to_string()),
        }
    }

    let initialized = paths.source_manifest_path().is_file();
    let source_state = if initialized {
        source::validate(paths).map_err(|error| format!("status: {error}"))?;
        Some(
            activation::inspect_library(&paths.source_command_dir)
                .map_err(|error| format!("status: source validation failed: {error}"))?,
        )
    } else {
        None
    };
    let current = activation::current(paths)?;
    let active_state = if paths.active_command_dir.is_dir() {
        Some(
            activation::inspect_library(&paths.active_command_dir)
                .map_err(|error| format!("status: active validation failed: {error}"))?,
        )
    } else {
        None
    };
    let pending_changes = match (&source_state, &active_state) {
        (Some(source), Some(active)) => source.digest != active.digest,
        (Some(source), None) => source.package_count > 0,
        (None, Some(active)) => active.package_count > 0,
        _ => false,
    };
    let output = StatusOutput {
        initialized,
        current_activation: current,
        source: source_state,
        active: active_state,
        pending_changes,
    };
    if json {
        write_json("status", &output)?;
    } else {
        println!("SCV status:");
        println!("  initialized        : {}", output.initialized);
        println!(
            "  current activation : {}",
            output.current_activation.as_deref().unwrap_or("none")
        );
        println!("  pending changes    : {}", output.pending_changes);
        if let Some(source) = output.source {
            println!("  source packages    : {}", source.package_count);
            println!("  source digest      : {}", source.digest);
        }
        if let Some(active) = output.active {
            println!("  active packages    : {}", active.package_count);
            println!("  active digest      : {}", active.digest);
        }
    }
    Ok(0)
}
