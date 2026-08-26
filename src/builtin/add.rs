use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::activation;
use crate::command::current_platform;
use crate::input;
use crate::metadata::{
    CommandMetadata, ImplementationMetadata, OptionMetadata, Registry, Risk, valid_name,
};
use crate::package;
use crate::paths::AppPaths;
use crate::source;

#[derive(Default)]
struct AddOptions {
    source: Option<PathBuf>,
    category: Option<String>,
    description: Option<String>,
    usage: Option<String>,
    name: Option<String>,
    runtime: Option<String>,
    platforms: Option<Vec<String>>,
    risk: Option<Risk>,
    network: Option<bool>,
    command_supports_dry_run: Option<bool>,
    effects: Vec<String>,
    force: bool,
    dry_run: bool,
    no_input: bool,
}

pub fn run(arguments: &[OsString], paths: &AppPaths, _registry: &Registry) -> Result<i32, String> {
    let mut options = parse(arguments)?;
    let registry = Registry::load_source(paths).map_err(|error| format!("add: {error}"))?;
    let source = options
        .source
        .take()
        .ok_or_else(|| usage_error("missing required argument 'source'"))?;
    if !source.is_file() {
        return Err(format!(
            "add: source file was not found: {}",
            source.display()
        ));
    }

    let source = fs::canonicalize(&source)
        .map_err(|error| format!("add: could not resolve '{}': {error}", source.display()))?;
    let name = options
        .name
        .take()
        .unwrap_or_else(|| default_name(&source).unwrap_or_default());
    if !valid_name(&name) {
        return Err(format!("add: invalid command name '{name}'"));
    }
    let existing = registry.get(&name);
    if existing.is_some_and(|metadata| metadata.builtin) {
        return Err(format!(
            "add: '{name}' is a built-in command and cannot be replaced"
        ));
    }
    let package_path = paths.source_package_dir(&name);
    if !options.force && existing.is_some() {
        return Err(format!(
            "add: command '{name}' already exists. Use --force to replace it"
        ));
    }
    if package_path.exists() && existing.is_none() {
        return Err(format!(
            "add: destination '{}' belongs to another or unregistered command",
            package_path.display()
        ));
    }

    let metadata = build_metadata(&mut options, &source, &name)?;
    let entry = &metadata.implementations[0].entry;
    let destination = paths.source_package_entry_path(&name, entry);
    let metadata_path = paths.source_package_metadata_path(&name);
    if options.dry_run {
        print_registration(
            "Would register",
            &metadata,
            &package_path,
            &destination,
            &metadata_path,
        );
        println!("No files were changed.");
        return Ok(0);
    }

    source::ensure_initialized(paths).map_err(|error| format!("add: {error}"))?;
    package::install_single_entry(&source, &metadata, paths, existing, options.force)
        .map_err(|error| format!("add: {error}"))?;
    let applied = activation::apply(paths).map_err(|error| {
        format!(
            "add: source was updated but activation failed; the previous activation remains active: {error}"
        )
    })?;
    print_registration(
        "Registered",
        &metadata,
        &package_path,
        &destination,
        &metadata_path,
    );
    println!("  activation: {}", applied.activation);
    Ok(0)
}

fn build_metadata(
    options: &mut AddOptions,
    source: &Path,
    name: &str,
) -> Result<CommandMetadata, String> {
    let interactive = input::is_enabled(options.no_input);
    let category = required_value(
        options.category.take(),
        "Category",
        "--category",
        interactive,
    )?;
    let description = required_value(
        options.description.take(),
        "Description",
        "--description",
        interactive,
    )?;
    let risk = required_risk(options.risk.take(), interactive)?;
    let network = required_bool(
        options.network.take(),
        "Network required",
        "--network",
        interactive,
    )?;
    let command_supports_dry_run = required_bool(
        options.command_supports_dry_run.take(),
        "Command supports --dry-run",
        "--supports-dry-run",
        interactive,
    )?;
    let effects = required_effects(std::mem::take(&mut options.effects), interactive)?;
    let runtime = options
        .runtime
        .take()
        .unwrap_or_else(|| infer_runtime(source).to_string());
    let platforms = options
        .platforms
        .take()
        .unwrap_or_else(|| vec![current_platform().to_string()]);
    let entry = destination_name(name, source, &runtime);
    let metadata = CommandMetadata {
        name: name.to_string(),
        category,
        description,
        usage: options
            .usage
            .take()
            .unwrap_or_else(|| format!("scv {name}")),
        builtin: false,
        runtime: String::new(),
        platforms: Vec::new(),
        entry: None,
        implementations: vec![ImplementationMetadata {
            runtime,
            platforms,
            entry: entry.clone(),
        }],
        risk,
        network,
        supports_dry_run: command_supports_dry_run,
        effects,
        arguments: Vec::new(),
        options: if command_supports_dry_run {
            vec![OptionMetadata {
                short: None,
                long: Some("--dry-run".to_string()),
                value: None,
                description: "reports the execution plan without making changes".to_string(),
                default: None,
            }]
        } else {
            Vec::new()
        },
        notes: Vec::new(),
        examples: Vec::new(),
    };
    metadata
        .validate()
        .map_err(|error| format!("add: {error}"))?;
    Ok(metadata)
}

fn parse(arguments: &[OsString]) -> Result<AddOptions, String> {
    let mut options = AddOptions::default();
    let mut index = 0;
    while index < arguments.len() {
        let argument = arguments[index]
            .to_str()
            .ok_or_else(|| "add: options must be valid UTF-8".to_string())?;
        match argument {
            "-c" | "--category" => set_option(
                &mut options.category,
                argument,
                value(arguments, &mut index, argument)?,
            )?,
            "-d" | "--description" => set_option(
                &mut options.description,
                argument,
                value(arguments, &mut index, argument)?,
            )?,
            "-u" | "--usage" => set_option(
                &mut options.usage,
                argument,
                value(arguments, &mut index, argument)?,
            )?,
            "-n" | "--name" => set_option(
                &mut options.name,
                argument,
                value(arguments, &mut index, argument)?,
            )?,
            "-r" | "--runtime" => set_option(
                &mut options.runtime,
                argument,
                value(arguments, &mut index, argument)?,
            )?,
            "-p" | "--platforms" => {
                let raw = value(arguments, &mut index, argument)?;
                let values = raw
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>();
                set_option(&mut options.platforms, argument, values)?;
            }
            "--risk" => {
                let risk = value(arguments, &mut index, argument)?;
                set_option(&mut options.risk, argument, parse_risk(&risk)?)?;
            }
            "--network" => {
                let raw = value(arguments, &mut index, argument)?;
                set_option(&mut options.network, argument, parse_bool(argument, &raw)?)?;
            }
            "--supports-dry-run" => {
                let raw = value(arguments, &mut index, argument)?;
                set_option(
                    &mut options.command_supports_dry_run,
                    argument,
                    parse_bool(argument, &raw)?,
                )?;
            }
            "--effect" => options
                .effects
                .push(value(arguments, &mut index, argument)?),
            "-f" | "--force" => set_flag(&mut options.force, argument)?,
            "--dry-run" => set_flag(&mut options.dry_run, argument)?,
            "--no-input" => set_flag(&mut options.no_input, argument)?,
            value if value.starts_with('-') => {
                return Err(usage_error(&format!("unknown option '{value}'")));
            }
            _ => {
                if options.source.is_some() {
                    return Err(usage_error(&format!("unexpected argument '{argument}'")));
                }
                options.source = Some(PathBuf::from(&arguments[index]));
            }
        }
        index += 1;
    }
    Ok(options)
}

fn set_option<T>(target: &mut Option<T>, option: &str, value: T) -> Result<(), String> {
    if target.is_some() {
        return Err(usage_error(&format!("duplicate option '{option}'")));
    }
    *target = Some(value);
    Ok(())
}

fn set_flag(target: &mut bool, option: &str) -> Result<(), String> {
    if *target {
        return Err(usage_error(&format!("duplicate option '{option}'")));
    }
    *target = true;
    Ok(())
}

fn value(arguments: &[OsString], index: &mut usize, option: &str) -> Result<String, String> {
    *index += 1;
    arguments
        .get(*index)
        .and_then(|value| value.to_str())
        .map(str::to_string)
        .ok_or_else(|| usage_error(&format!("{option} requires a value")))
}

fn required_value(
    value: Option<String>,
    label: &str,
    option: &str,
    interactive: bool,
) -> Result<String, String> {
    let value = match value {
        Some(value) => value,
        None if !interactive => {
            return Err(format!(
                "add: missing required option '{option}' because input is disabled"
            ));
        }
        None => {
            print!("{label}: ");
            io::stdout()
                .flush()
                .map_err(|error| format!("add: could not write prompt: {error}"))?;
            let mut input = String::new();
            io::stdin()
                .read_line(&mut input)
                .map_err(|error| format!("add: could not read {label}: {error}"))?;
            input.trim().to_string()
        }
    };
    if value.trim().is_empty() {
        Err(format!("add: {} cannot be empty", label.to_lowercase()))
    } else {
        Ok(value)
    }
}

fn required_risk(value: Option<Risk>, interactive: bool) -> Result<Risk, String> {
    match value {
        Some(value) => Ok(value),
        None if !interactive => {
            Err("add: missing required option '--risk' because input is disabled".to_string())
        }
        None => {
            let value = required_value(None, "Risk (read/write/destructive)", "--risk", true)?;
            parse_risk(&value)
        }
    }
}

fn required_bool(
    value: Option<bool>,
    label: &str,
    option: &str,
    interactive: bool,
) -> Result<bool, String> {
    match value {
        Some(value) => Ok(value),
        None if !interactive => Err(format!(
            "add: missing required option '{option}' because input is disabled"
        )),
        None => {
            let value = required_value(None, &format!("{label} (true/false)"), option, true)?;
            parse_bool(option, &value)
        }
    }
}

fn required_effects(mut effects: Vec<String>, interactive: bool) -> Result<Vec<String>, String> {
    if effects.is_empty() {
        if !interactive {
            return Err(
                "add: at least one --effect is required because input is disabled".to_string(),
            );
        }
        effects.push(required_value(None, "Effect", "--effect", true)?);
    }
    Ok(effects)
}

fn parse_risk(value: &str) -> Result<Risk, String> {
    match value {
        "read" => Ok(Risk::Read),
        "write" => Ok(Risk::Write),
        "destructive" => Ok(Risk::Destructive),
        _ => Err(format!(
            "add: invalid risk '{value}'; expected read, write, or destructive"
        )),
    }
}

fn parse_bool(option: &str, value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!(
            "add: {option} expects true or false, got '{value}'"
        )),
    }
}

fn print_registration(
    action: &str,
    metadata: &CommandMetadata,
    package_path: &Path,
    destination: &Path,
    metadata_path: &Path,
) {
    let implementation = &metadata.implementations[0];
    println!("{action}: scv {}", metadata.name);
    println!("  category  : {}", metadata.category);
    println!("  runtime   : {}", implementation.runtime);
    println!("  platforms : {}", implementation.platforms.join(", "));
    println!("  risk      : {}", metadata.risk.as_str());
    println!("  network   : {}", metadata.network);
    println!("  package   : {}", package_path.display());
    println!("  entry     : {}", destination.display());
    println!("  metadata  : {}", metadata_path.display());
}

fn default_name(source: &Path) -> Option<String> {
    source.file_stem()?.to_str().map(str::to_string)
}

fn infer_runtime(source: &Path) -> &'static str {
    match source
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("sh" | "bash") => "bash",
        Some("js" | "mjs" | "cjs") => "node",
        Some("py") => "python",
        Some("ps1") => "pwsh",
        Some("exe") => "binary",
        _ => infer_shebang(source).unwrap_or("binary"),
    }
}

fn infer_shebang(source: &Path) -> Option<&'static str> {
    let mut file = fs::File::open(source).ok()?;
    let mut prefix = [0_u8; 256];
    let length = file.read(&mut prefix).ok()?;
    let first_line = prefix[..length].split(|byte| *byte == b'\n').next()?;
    shebang_runtime(&String::from_utf8_lossy(first_line))
}

fn shebang_runtime(line: &str) -> Option<&'static str> {
    let interpreter = line.strip_prefix("#!")?.to_ascii_lowercase();
    if interpreter.contains("bash") {
        Some("bash")
    } else if interpreter.contains("node") {
        Some("node")
    } else if interpreter.contains("python") {
        Some("python")
    } else if interpreter.contains("pwsh") || interpreter.contains("powershell") {
        Some("pwsh")
    } else {
        None
    }
}

fn destination_name(name: &str, source: &Path, runtime: &str) -> String {
    let extension = source.extension().and_then(|value| value.to_str());
    if matches!(runtime, "node" | "python" | "pwsh")
        && let Some(extension) = extension
    {
        return format!("{name}.{extension}");
    }
    if runtime == "binary" && extension.is_some_and(|value| value.eq_ignore_ascii_case("exe")) {
        return format!("{name}.exe");
    }
    name.to_string()
}

fn usage_error(message: &str) -> String {
    format!("add: {message}. Try 'scv add --help' for more information")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{parse, shebang_runtime};

    #[test]
    fn rejects_duplicate_options_and_flags() {
        for arguments in [
            vec!["--category", "one", "-c", "two"],
            vec!["--dry-run", "--dry-run"],
            vec!["--no-input", "--no-input"],
        ] {
            let arguments = arguments
                .into_iter()
                .map(OsString::from)
                .collect::<Vec<_>>();
            let Err(error) = parse(&arguments) else {
                panic!("duplicate option should fail");
            };
            assert!(error.contains("duplicate option"));
        }
    }

    #[test]
    fn infers_runtime_only_from_an_actual_shebang() {
        assert_eq!(shebang_runtime("#!/usr/bin/env bash"), Some("bash"));
        assert_eq!(shebang_runtime("#!/usr/bin/env python3"), Some("python"));
        assert_eq!(shebang_runtime("echo bash"), None);
    }
}
