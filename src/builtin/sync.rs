use std::ffi::OsString;
use std::io::{self, Write};

use crate::input;
use crate::paths::AppPaths;
use crate::sync;

use super::write_json;

pub fn run(arguments: &[OsString], paths: &AppPaths) -> Result<i32, String> {
    let mut operation = None;
    let mut yes = false;
    let mut no_input = false;
    let mut dry_run = false;
    let mut json = false;
    for argument in arguments {
        let argument = argument
            .to_str()
            .ok_or_else(|| "sync: arguments must be valid UTF-8".to_string())?;
        match argument {
            "-y" | "--yes" if !yes => yes = true,
            "--no-input" if !no_input => no_input = true,
            "--dry-run" if !dry_run => dry_run = true,
            "--json" if !json => json = true,
            value if value.starts_with('-') => {
                return Err(format!("sync: unknown or duplicate option '{value}'"));
            }
            value if operation.is_none() => operation = Some(value),
            value => return Err(format!("sync: unexpected argument '{value}'")),
        }
    }
    match operation.unwrap_or("status") {
        "status" => {
            reject_mutation_options(yes, no_input, dry_run)?;
            let status = sync::status(paths).map_err(|error| format!("sync: {error}"))?;
            if json {
                write_json("sync", &status)?;
            } else {
                println!("SCV Git source:");
                println!("  repository : {}", status.repository);
                println!("  branch     : {}", status.branch);
                println!("  head       : {}", status.head);
                println!(
                    "  upstream   : {}",
                    status.upstream.as_deref().unwrap_or("none")
                );
                println!("  dirty      : {}", status.dirty);
                println!("  ahead      : {}", status.ahead);
                println!("  behind     : {}", status.behind);
            }
        }
        "pull" => {
            if dry_run {
                return Err(
                    "sync: pull does not support --dry-run because fetch updates Git refs"
                        .to_string(),
                );
            }
            let preview = sync::prepare_pull(paths).map_err(|error| format!("sync: {error}"))?;
            if preview.from == preview.to {
                if json {
                    let output = serde_json::json!({
                        "changed": false,
                        "from": preview.from,
                        "to": preview.to,
                        "activation": null,
                    });
                    write_json("sync", &output)?;
                } else {
                    println!("Already up to date.");
                }
                return Ok(0);
            }
            if !json {
                println!("Remote changes validated in an isolated worktree:");
                println!("  from : {}", preview.from);
                println!("  to   : {}", preview.to);
                if !preview.summary.is_empty() {
                    println!();
                    println!("{}", preview.summary);
                }
            }
            let approved = if yes {
                true
            } else {
                match confirm(no_input, "Apply remote changes and activate them?") {
                    Ok(approved) => approved,
                    Err(error) => {
                        sync::cleanup_preview(paths, &preview);
                        return Err(error);
                    }
                }
            };
            if !approved {
                sync::cleanup_preview(paths, &preview);
                println!("Cancelled. The active command library was not changed.");
                return Ok(0);
            }
            let result =
                sync::commit_pull(paths, preview).map_err(|error| format!("sync: {error}"))?;
            if json {
                write_json("sync", &result)?;
            } else {
                println!("Source updated and activated: {}", result.to);
            }
        }
        "push" => {
            if !dry_run && !yes && !confirm(no_input, "Push committed SCV source changes?")? {
                println!("Cancelled. No remote changes were made.");
                return Ok(0);
            }
            sync::push(paths, dry_run).map_err(|error| format!("sync: {error}"))?;
            if json {
                let output = serde_json::json!({ "pushed": !dry_run, "dry_run": dry_run });
                write_json("sync", &output)?;
            } else if dry_run {
                println!("Git push dry-run completed. No remote changes were made.");
            } else {
                println!("Committed SCV source changes were pushed.");
            }
        }
        value => return Err(format!("sync: unknown operation '{value}'")),
    }
    Ok(0)
}

fn reject_mutation_options(yes: bool, no_input: bool, dry_run: bool) -> Result<(), String> {
    if yes || no_input || dry_run {
        Err("sync: status does not accept mutation options".to_string())
    } else {
        Ok(())
    }
}

fn confirm(no_input: bool, question: &str) -> Result<bool, String> {
    if !input::is_enabled(no_input) {
        return Err("sync: --yes is required when input is disabled".to_string());
    }
    print!("{question} [y/N] ");
    io::stdout()
        .flush()
        .map_err(|error| format!("sync: could not write prompt: {error}"))?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|error| format!("sync: could not read confirmation: {error}"))?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
