use std::ffi::OsString;

use crate::activation;
use crate::paths::AppPaths;

use super::write_json;

pub fn run(arguments: &[OsString], paths: &AppPaths) -> Result<i32, String> {
    let mut activation_id = None;
    let mut dry_run = false;
    let mut json = false;
    for argument in arguments {
        let argument = argument
            .to_str()
            .ok_or_else(|| "rollback: arguments must be valid UTF-8".to_string())?;
        match argument {
            "--dry-run" if !dry_run => dry_run = true,
            "--json" if !json => json = true,
            value if value.starts_with('-') => {
                return Err(format!("rollback: unknown or duplicate option '{value}'"));
            }
            value if activation_id.is_none() => activation_id = Some(value),
            value => return Err(format!("rollback: unexpected argument '{value}'")),
        }
    }

    if dry_run {
        let preview = activation::preview_rollback(paths, activation_id)?;
        let plan = serde_json::json!({
            "current": &preview.previous,
            "target": &preview.activation,
            "package_count": preview.package_count,
            "digest": preview.digest,
            "changed": false,
        });
        if json {
            write_json("rollback", &plan)?;
        } else {
            println!("Would switch activation to: {}", preview.activation);
            println!("No files were changed.");
        }
        return Ok(0);
    }

    let result = activation::rollback(paths, activation_id)?;
    if json {
        write_json("rollback", &result)?;
    } else {
        println!("Activation switched:");
        println!("  from : {}", result.previous.as_deref().unwrap_or("none"));
        println!("  to   : {}", result.activation);
    }
    Ok(0)
}
