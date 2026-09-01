use serde::Serialize;
use std::ffi::OsString;

use crate::paths::AppPaths;

use super::write_json;

#[derive(Serialize)]
struct PathsOutput {
    source_home: String,
    source_commands: String,
    data: String,
    activations: String,
    history: String,
    state: String,
    current: String,
    active_commands: String,
    config: String,
    cache: String,
}

pub fn run(arguments: &[OsString], paths: &AppPaths) -> Result<i32, String> {
    let mut json = false;
    for argument in arguments {
        match argument.to_str() {
            Some("--json") if !json => json = true,
            Some(value) => return Err(format!("paths: unknown or duplicate option '{value}'")),
            None => return Err("paths: options must be valid UTF-8".to_string()),
        }
    }
    let output = PathsOutput {
        source_home: display(&paths.source_home),
        source_commands: display(&paths.source_command_dir),
        data: display(&paths.data_dir),
        activations: display(&paths.activations_dir),
        history: display(&paths.history_dir),
        state: display(&paths.state_dir),
        current: display(&paths.current_path),
        active_commands: display(&paths.active_command_dir),
        config: display(&paths.config_dir),
        cache: display(&paths.cache_dir),
    };
    if json {
        write_json("paths", &output)?;
    } else {
        println!("SCV paths:");
        println!("  source home     : {}", output.source_home);
        println!("  source commands : {}", output.source_commands);
        println!("  data            : {}", output.data);
        println!("  activations     : {}", output.activations);
        println!("  history         : {}", output.history);
        println!("  state           : {}", output.state);
        println!("  current         : {}", output.current);
        println!("  active commands : {}", output.active_commands);
        println!("  config          : {}", output.config);
        println!("  cache           : {}", output.cache);
    }
    Ok(0)
}

fn display(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}
