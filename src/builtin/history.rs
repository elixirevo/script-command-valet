use std::ffi::OsString;
use std::io::{self, Write};

use crate::command;
use crate::history;
use crate::i18n::I18n;
use crate::input;
use crate::paths::AppPaths;

use super::write_json;

#[derive(Default)]
struct Options {
    action: Option<String>,
    id: Option<String>,
    json: bool,
    yes: bool,
    no_input: bool,
}

pub fn run(arguments: &[OsString], paths: &AppPaths, i18n: &I18n) -> Result<i32, String> {
    let options = parse(arguments)?;
    match options.action.as_deref().unwrap_or("list") {
        "list" => list_entries(options, paths, i18n),
        "run" => run_entry(options, paths, i18n),
        action => Err(usage_error(&format!("unknown action '{action}'"))),
    }
}

fn list_entries(options: Options, paths: &AppPaths, i18n: &I18n) -> Result<i32, String> {
    if options.id.is_some() || options.yes || options.no_input {
        return Err(usage_error(
            "list does not accept an ID, --yes, or --no-input",
        ));
    }
    let entries = history::list(paths)?;
    if options.json {
        write_json("history", &entries)?;
        return Ok(0);
    }
    if entries.is_empty() {
        println!("{}", i18n.text("history.empty"));
        return Ok(0);
    }
    for entry in entries {
        println!("{}  {}", entry.id, entry.request);
        println!(
            "  {}: {}  {}: {}  {}: {}",
            i18n.text("history.agent"),
            entry.agent,
            i18n.text("history.risk"),
            entry.risk.as_str(),
            i18n.text("history.network"),
            entry.network,
        );
        println!(
            "  {}: {}",
            i18n.text("history.working_directory"),
            entry.working_directory
        );
    }
    Ok(0)
}

fn run_entry(options: Options, paths: &AppPaths, i18n: &I18n) -> Result<i32, String> {
    if options.json {
        return Err(usage_error("run does not support --json"));
    }
    let id = options
        .id
        .as_deref()
        .ok_or_else(|| usage_error("run requires a history ID"))?;
    let stored = history::load(id, paths)?;
    print_preview(&stored, i18n);
    let interactive = input::is_enabled(options.no_input);
    if !options.yes && !interactive {
        return Err(
            "history: --yes is required when input is disabled; the command was not run"
                .to_string(),
        );
    }
    if !options.yes && !confirm(i18n)? {
        println!("{}", i18n.text("common.cancelled"));
        return Ok(0);
    }
    println!("{}", i18n.text("history.running"));
    command::run_package(&stored.package.metadata, &[], &stored.package_dir, paths)
}

fn print_preview(stored: &history::StoredHistory, i18n: &I18n) {
    println!("{}", i18n.text("history.preview"));
    println!("  id          : {}", stored.summary.id);
    println!("  request     : {}", stored.summary.request);
    println!("  command     : {}", stored.summary.command);
    println!("  description : {}", stored.summary.description);
    println!("  risk        : {}", stored.summary.risk.as_str());
    println!("  network     : {}", stored.summary.network);
    println!("  effects:");
    for effect in &stored.package.metadata.effects {
        println!("    - {effect}");
    }
    println!(
        "  {}: {}",
        i18n.text("history.original_working_directory"),
        stored.summary.working_directory
    );
    println!(
        "  {}: {}",
        i18n.text("history.execution_working_directory"),
        std::env::current_dir()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "<unavailable>".to_string())
    );
}

fn parse(arguments: &[OsString]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut positionals = Vec::new();
    for argument in arguments {
        match argument.to_str() {
            Some("--json") if !options.json => options.json = true,
            Some("-y" | "--yes") if !options.yes => options.yes = true,
            Some("--no-input") if !options.no_input => options.no_input = true,
            Some(value) if value.starts_with('-') => {
                return Err(usage_error(&format!(
                    "unknown or duplicate option '{value}'"
                )));
            }
            Some(value) => positionals.push(value.to_string()),
            None => return Err("history: arguments must be valid UTF-8".to_string()),
        }
    }
    match positionals.as_slice() {
        [] => {}
        [action] => options.action = Some(action.clone()),
        [action, id] => {
            options.action = Some(action.clone());
            options.id = Some(id.clone());
        }
        _ => return Err(usage_error("too many arguments")),
    }
    Ok(options)
}

fn confirm(i18n: &I18n) -> Result<bool, String> {
    print!("{}", i18n.text("history.confirm"));
    io::stdout()
        .flush()
        .map_err(|error| format!("history: could not write prompt: {error}"))?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|error| format!("history: could not read confirmation: {error}"))?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn usage_error(message: &str) -> String {
    format!("history: {message}. Try 'scv history --help' for more information")
}

#[cfg(test)]
mod tests {
    use super::parse;
    use std::ffi::OsString;

    #[test]
    fn parses_list_and_run_forms() {
        let list = parse(&[OsString::from("--json")]).expect("list should parse");
        assert!(list.json);
        let run = parse(&["run", "123-id", "--yes", "--no-input"].map(OsString::from))
            .expect("run should parse");
        assert_eq!(run.action.as_deref(), Some("run"));
        assert_eq!(run.id.as_deref(), Some("123-id"));
        assert!(run.yes && run.no_input);
    }

    #[test]
    fn rejects_unknown_options() {
        assert!(parse(&[OsString::from("--all")]).is_err());
    }
}
