use std::ffi::OsString;

use crate::activation;
use crate::paths::AppPaths;
use crate::source;

use super::write_json;

pub fn run(arguments: &[OsString], paths: &AppPaths) -> Result<i32, String> {
    let mut dry_run = false;
    let mut json = false;
    for argument in arguments {
        match argument.to_str() {
            Some("--dry-run") if !dry_run => dry_run = true,
            Some("--json") if !json => json = true,
            Some(value) => return Err(format!("apply: unknown or duplicate option '{value}'")),
            None => return Err("apply: options must be valid UTF-8".to_string()),
        }
    }

    if dry_run {
        source::validate(paths).map_err(|error| format!("apply: {error}"))?;
        let state = activation::inspect_library(&paths.source_command_dir)
            .map_err(|error| format!("apply: validation failed: {error}"))?;
        if json {
            write_json("apply", &state)?;
        } else {
            println!("Source is valid:");
            println!("  packages : {}", state.package_count);
            println!("  digest   : {}", state.digest);
            println!("No activation was created.");
        }
        return Ok(0);
    }

    let result = activation::apply(paths).map_err(|error| format!("apply: {error}"))?;
    if json {
        write_json("apply", &result)?;
    } else {
        println!("Activated SCV command library:");
        println!("  activation : {}", result.activation);
        println!("  packages   : {}", result.package_count);
        println!("  digest     : {}", result.digest);
    }
    Ok(0)
}
