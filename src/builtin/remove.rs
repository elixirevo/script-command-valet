use std::ffi::OsString;
use std::io::{self, Write};

use crate::activation;
use crate::input;
use crate::metadata::Registry;
use crate::paths::AppPaths;
use crate::source;
use crate::storage;

pub fn run(arguments: &[OsString], paths: &AppPaths, _registry: &Registry) -> Result<i32, String> {
    let registry = Registry::load_source(paths).map_err(|error| format!("rm: {error}"))?;
    let mut name = None;
    let mut yes = false;
    let mut dry_run = false;
    let mut no_input = false;
    for argument in arguments {
        let argument = argument
            .to_str()
            .ok_or_else(|| "rm: arguments must be valid UTF-8".to_string())?;
        match argument {
            "-y" | "--yes" if !yes => yes = true,
            "--dry-run" if !dry_run => dry_run = true,
            "--no-input" if !no_input => no_input = true,
            value if value.starts_with('-') => {
                return Err(format!("rm: unknown or duplicate option '{value}'"));
            }
            value if name.is_none() => name = Some(value),
            value => return Err(format!("rm: unexpected argument '{value}'")),
        }
    }
    let name =
        name.ok_or_else(|| "rm: missing required command name. Try 'scv rm --help'".to_string())?;
    let metadata = registry
        .get(name)
        .ok_or_else(|| format!("rm: '{name}' is not a registered command"))?;
    if metadata.builtin {
        return Err(format!(
            "rm: '{name}' is a built-in command and cannot be removed"
        ));
    }
    println!("Remove command:");
    println!("  command  : {name}");
    println!("  package  : {}", paths.source_package_dir(name).display());
    println!(
        "  metadata : {}",
        paths.source_package_metadata_path(name).display()
    );
    if dry_run {
        println!("No files were removed.");
        return Ok(0);
    }
    if !yes {
        if !input::is_enabled(no_input) {
            return Err("rm: --yes is required when input is disabled".to_string());
        }
        print!("Continue? [y/N] ");
        io::stdout()
            .flush()
            .map_err(|error| format!("rm: could not write prompt: {error}"))?;
        let mut answer = String::new();
        io::stdin()
            .read_line(&mut answer)
            .map_err(|error| format!("rm: could not read confirmation: {error}"))?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            println!("Cancelled.");
            return Ok(0);
        }
    }

    source::validate(paths).map_err(|error| format!("rm: {error}"))?;
    remove_path(&paths.source_package_dir(name))?;
    let applied = activation::apply(paths).map_err(|error| {
        format!(
            "rm: source was updated but activation failed; the previous activation remains active: {error}"
        )
    })?;
    println!("Removed: {name}");
    println!("Activation: {}", applied.activation);
    Ok(0)
}

fn remove_path(path: &std::path::Path) -> Result<(), String> {
    storage::remove_if_exists(path).map_err(|error| format!("rm: {error}"))
}
