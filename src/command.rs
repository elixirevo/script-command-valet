use std::ffi::OsString;
use std::fs;
use std::process::Command;

use crate::metadata::CommandMetadata;
use crate::paths::AppPaths;

pub fn current_platform() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

pub fn platform_label(platform: &str) -> &str {
    match platform {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    }
}

pub fn run_external(
    metadata: &CommandMetadata,
    arguments: &[OsString],
    paths: &AppPaths,
) -> Result<i32, String> {
    crate::activation::verify_current_package(paths, &metadata.name)?;
    run_package(
        metadata,
        arguments,
        &paths.active_package_dir(&metadata.name),
        paths,
    )
}

pub fn run_package(
    metadata: &CommandMetadata,
    arguments: &[OsString],
    package_dir: &std::path::Path,
    paths: &AppPaths,
) -> Result<i32, String> {
    let platform = current_platform();
    let implementation = metadata.implementation_for(platform).ok_or_else(|| {
        let supported = metadata
            .supported_platforms()
            .iter()
            .map(|value| platform_label(value))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "'{}' is not supported on {}. Supported platforms: {supported}",
            metadata.name,
            platform_label(platform)
        )
    })?;

    let entry_path = package_dir.join(implementation.entry);
    let entry_metadata = fs::symlink_metadata(&entry_path).map_err(|error| {
        format!(
            "entry for '{}' was not found or could not be inspected: {}: {error}",
            metadata.name,
            entry_path.display()
        )
    })?;
    if !entry_metadata.is_file() || entry_metadata.file_type().is_symlink() {
        return Err(format!(
            "entry for '{}' must be a regular non-symlink file in command storage: {}",
            metadata.name,
            entry_path.display()
        ));
    }

    let mut process = match implementation.runtime {
        "binary" => Command::new(&entry_path),
        "bash" => interpreter("bash", &entry_path),
        "node" => interpreter("node", &entry_path),
        "python" => interpreter(python_runtime(), &entry_path),
        "pwsh" => interpreter("pwsh", &entry_path),
        runtime => return Err(format!("unsupported runtime '{runtime}'")),
    };

    process.args(arguments);
    paths.apply_to(&mut process);
    process
        .env("SCV_COMMAND_PACKAGE_DIR", package_dir)
        .env("SCV_COMMAND_METADATA", package_dir.join("metadata.toml"));

    let status = process.status().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!(
                "runtime '{}' required by '{}' was not found",
                implementation.runtime, metadata.name
            )
        } else {
            format!("could not run '{}': {error}", metadata.name)
        }
    })?;

    Ok(status.code().unwrap_or(1))
}

fn interpreter(program: &str, entry: &std::path::Path) -> Command {
    let mut command = Command::new(program);
    command.arg(entry);
    command
}

pub fn python_runtime() -> &'static str {
    if cfg!(target_os = "windows") {
        "python"
    } else {
        "python3"
    }
}
