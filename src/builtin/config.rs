use std::ffi::OsString;

use crate::agent::{self, Effort};
use crate::config::Settings;
use crate::paths::AppPaths;

use super::write_json;

pub fn run(arguments: &[OsString], paths: &AppPaths) -> Result<i32, String> {
    let mut arguments = arguments.iter();
    let action = arguments
        .next()
        .and_then(|value| value.to_str())
        .unwrap_or("show");
    match action {
        "show" => show(arguments, paths),
        "set" => set(arguments, paths),
        value => Err(usage_error(&format!("unknown action '{value}'"))),
    }
}

fn show<'a>(
    arguments: impl Iterator<Item = &'a OsString>,
    paths: &AppPaths,
) -> Result<i32, String> {
    let mut json = false;
    for argument in arguments {
        match argument.to_str() {
            Some("--json") if !json => json = true,
            Some(value) => {
                return Err(usage_error(&format!(
                    "unknown or duplicate option '{value}'"
                )));
            }
            None => return Err("config: options must be valid UTF-8".to_string()),
        }
    }
    let settings = Settings::load(paths)?;
    if json {
        write_json("config", &settings)?;
    } else {
        println!("Config: {}", paths.config_path().display());
        let contents = toml::to_string_pretty(&settings)
            .map_err(|error| format!("config: could not serialize config: {error}"))?;
        if contents.trim().is_empty() {
            println!("(using agent CLI defaults; create agent defaults to codex)");
        } else {
            print!("{contents}");
        }
    }
    Ok(0)
}

fn set<'a>(arguments: impl Iterator<Item = &'a OsString>, paths: &AppPaths) -> Result<i32, String> {
    let arguments = arguments.collect::<Vec<_>>();
    let mut dry_run = false;
    let mut values = Vec::new();
    for argument in arguments {
        let argument = argument
            .to_str()
            .ok_or_else(|| "config: arguments must be valid UTF-8".to_string())?;
        match argument {
            "--dry-run" if !dry_run => dry_run = true,
            value if value.starts_with('-') => {
                return Err(usage_error(&format!(
                    "unknown or duplicate option '{value}'"
                )));
            }
            value => values.push(value),
        }
    }
    let [key, value] = values.as_slice() else {
        return Err(usage_error("set requires exactly <key> and <value>"));
    };
    validate_value(key, value)?;
    let mut settings = Settings::load(paths)?;
    settings.set(key, (*value).to_string())?;
    if dry_run {
        println!("Would set {key} = {value}");
        println!("Config: {}", paths.config_path().display());
        println!("No files were changed.");
        return Ok(0);
    }
    settings.save(paths)?;
    println!("Set {key} = {value}");
    println!("Config: {}", paths.config_path().display());
    Ok(0)
}

fn validate_value(key: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.contains(['\n', '\r']) {
        return Err(format!(
            "config: value for '{key}' must be a non-empty line"
        ));
    }
    if matches!(key, "agent" | "create.agent") {
        agent::adapter(value).map(|_| ())?;
    }
    if key == "effort" || key.ends_with(".effort") {
        Effort::parse(value).map(|_| ())?;
    }
    if let Some(option) = key.split(".options.").nth(1) {
        agent::validate_option_key(option).map_err(|error| format!("config: {error}"))?;
    }
    Ok(())
}

fn usage_error(message: &str) -> String {
    format!("config: {message}. Try 'scv config --help' for more information")
}

#[cfg(test)]
mod tests {
    use super::validate_value;

    #[test]
    fn validates_known_agents_and_portable_effort() {
        assert!(validate_value("agent", "codex").is_ok());
        assert!(validate_value("agent", "unknown").is_err());
        assert!(validate_value("effort", "high").is_ok());
        assert!(validate_value("effort", "auto").is_err());
    }
}
