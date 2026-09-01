use std::ffi::OsString;

use serde::Serialize;

use crate::command::current_platform;
use crate::help;
use crate::i18n::I18n;
use crate::metadata::{Registry, Risk};

use super::write_json;

#[derive(Serialize)]
struct CommandSummary<'a> {
    name: &'a str,
    category: &'a str,
    description: &'a str,
    runtime: String,
    platforms: Vec<String>,
    available: bool,
    risk: Risk,
    network: bool,
    supports_dry_run: bool,
}

pub fn run(arguments: &[OsString], registry: &Registry, i18n: &I18n) -> Result<i32, String> {
    let mut show_all = false;
    let mut json = false;
    for argument in arguments {
        match argument.to_str() {
            Some("-a" | "--all") if !show_all => show_all = true,
            Some("--json") if !json => json = true,
            Some(value) => return Err(format!("list: unknown or duplicate option '{value}'")),
            None => return Err("list: options must be valid UTF-8".to_string()),
        }
    }

    let platform = current_platform();
    let commands = registry
        .iter()
        .filter(|command| show_all || command.supports(platform))
        .collect::<Vec<_>>();

    if json {
        let summaries = commands
            .iter()
            .map(|command| CommandSummary {
                name: &command.name,
                category: &command.category,
                description: &command.description,
                runtime: command.runtime_label(platform),
                platforms: command.supported_platforms(),
                available: command.supports(platform),
                risk: command.risk,
                network: command.network,
                supports_dry_run: command.supports_dry_run,
            })
            .collect::<Vec<_>>();
        write_json("list", &summaries)?;
        return Ok(0);
    }

    for (index, (category, commands)) in help::grouped_by_category(commands).into_iter().enumerate()
    {
        if index > 0 {
            println!();
        }
        println!("{category}:");
        for command in commands {
            println!(
                "  {:<20} {}",
                command.name,
                i18n.command_description(command)
            );
        }
    }
    Ok(0)
}
