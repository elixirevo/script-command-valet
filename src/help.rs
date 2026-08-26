use std::collections::BTreeMap;

use crate::command::{current_platform, platform_label};
use crate::metadata::{CommandMetadata, Registry};

pub fn print_top(registry: &Registry) {
    let platform = current_platform();

    println!("Usage:");
    println!("  scv <command> [arguments]");
    println!("  scv <command> --help");
    println!("  scv --help");
    println!();
    println!("Available commands:");

    for (category, commands) in
        grouped_by_category(registry.iter().filter(|command| command.supports(platform)))
    {
        println!();
        println!("{category}:");
        for command in commands {
            println!("  {:<20} {}", command.name, command.description);
        }
    }
}

pub fn grouped_by_category<'a>(
    commands: impl IntoIterator<Item = &'a CommandMetadata>,
) -> BTreeMap<&'a str, Vec<&'a CommandMetadata>> {
    let mut categories: BTreeMap<&str, Vec<&CommandMetadata>> = BTreeMap::new();
    for command in commands {
        categories
            .entry(&command.category)
            .or_default()
            .push(command);
    }
    for commands in categories.values_mut() {
        commands.sort_by(|left, right| left.name.cmp(&right.name));
    }
    categories
}

pub fn print_command(command: &CommandMetadata) {
    println!("Usage:");
    println!("  {}", command.usage);
    println!();
    println!("Description:");
    println!("  {}", command.description);
    println!();
    println!("Safety:");
    println!("  risk        : {}", command.risk.as_str());
    println!(
        "  network     : {}",
        if command.network { "required" } else { "no" }
    );
    println!(
        "  dry-run     : {}",
        if command.supports_dry_run {
            "supported"
        } else {
            "not supported"
        }
    );
    println!("  effects:");
    for effect in &command.effects {
        println!("    - {effect}");
    }

    if !command.notes.is_empty() {
        println!();
        println!("Notes:");
        for note in &command.notes {
            println!("  - {}", note.text);
        }
    }

    if !command.arguments.is_empty() {
        println!();
        println!("Arguments:");
        for argument in &command.arguments {
            let default = default_suffix(argument.default.as_deref());
            println!(
                "  {:<34} {}{}",
                argument.name, argument.description, default
            );
        }
    }

    println!();
    println!("Options:");
    for option in &command.options {
        let mut label = match (&option.short, &option.long) {
            (Some(short), Some(long)) => format!("{short}, {long}"),
            (Some(short), None) => short.clone(),
            (None, Some(long)) => long.clone(),
            (None, None) => continue,
        };
        if let Some(value) = &option.value {
            label.push(' ');
            label.push_str(value);
        }
        let default = default_suffix(option.default.as_deref());
        println!("  {label:<34} {}{}", option.description, default);
    }
    println!("  {:<34} 도움말 표시", "-h, --help");

    if !command.examples.is_empty() {
        println!();
        println!("Examples:");
        for example in &command.examples {
            println!("  {}", example.command);
        }
    }
}

fn default_suffix(value: Option<&str>) -> String {
    value
        .map(|value| format!(" (default: {value})"))
        .unwrap_or_default()
}

pub fn print_info(command: &CommandMetadata) {
    print_command(command);
    if command.implementations.is_empty() {
        println!();
        println!("Runtime:");
        println!("  {}", command.runtime);
        println!();
        println!("Platforms:");
        println!(
            "  {}",
            command
                .supported_platforms()
                .iter()
                .map(|platform| platform_label(platform))
                .collect::<Vec<_>>()
                .join(", ")
        );
        if let Some(entry) = &command.entry {
            println!();
            println!("Entry:");
            println!("  {entry}");
        }
    } else {
        println!();
        println!("Implementations:");
        for implementation in &command.implementations {
            let platforms = implementation
                .platforms
                .iter()
                .map(|platform| platform_label(platform))
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "  {:<20} {:<8} {}",
                platforms, implementation.runtime, implementation.entry
            );
        }
    }
}
