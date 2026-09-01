use std::ffi::OsString;

use crate::agent::{self, Effort};
use crate::config::Settings;
use crate::i18n::{I18n, Locale};
use crate::paths::AppPaths;

use super::write_json;

pub fn run(arguments: &[OsString], paths: &AppPaths, i18n: &I18n) -> Result<i32, String> {
    let mut arguments = arguments.iter();
    let action = arguments
        .next()
        .and_then(|value| value.to_str())
        .unwrap_or("show");
    match action {
        "show" => show(arguments, paths, i18n),
        "set" => set(arguments, paths, i18n),
        value => Err(usage_error(&format!("unknown action '{value}'"))),
    }
}

fn show<'a>(
    arguments: impl Iterator<Item = &'a OsString>,
    paths: &AppPaths,
    i18n: &I18n,
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
        println!(
            "{}",
            i18n.format(
                "config.path",
                &[("path", &paths.config_path().display().to_string())]
            )
        );
        let contents = toml::to_string_pretty(&settings)
            .map_err(|error| format!("config: could not serialize config: {error}"))?;
        if contents.trim().is_empty() {
            println!("{}", i18n.text("config.defaults"));
        } else {
            print!("{contents}");
        }
    }
    Ok(0)
}

fn set<'a>(
    arguments: impl Iterator<Item = &'a OsString>,
    paths: &AppPaths,
    i18n: &I18n,
) -> Result<i32, String> {
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
        println!(
            "{}",
            i18n.format("config.would_set", &[("key", key), ("value", value)])
        );
        println!(
            "{}",
            i18n.format(
                "config.path",
                &[("path", &paths.config_path().display().to_string())]
            )
        );
        println!("{}", i18n.text("common.no_files_changed"));
        return Ok(0);
    }
    settings.save(paths)?;
    let output_i18n = if *key == "ui.locale" {
        I18n::new(Locale::parse(value)?)?
    } else {
        I18n::new(i18n.locale())?
    };
    println!(
        "{}",
        output_i18n.format("config.set", &[("key", key), ("value", value)])
    );
    println!(
        "{}",
        output_i18n.format(
            "config.path",
            &[("path", &paths.config_path().display().to_string())]
        )
    );
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
    if key == "ui.locale" {
        Locale::parse(value)?;
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
        assert!(validate_value("agent", "agy").is_ok());
        assert!(validate_value("agent", "unknown").is_err());
        assert!(validate_value("effort", "high").is_ok());
        assert!(validate_value("effort", "auto").is_err());
    }
}
