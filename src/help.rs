use std::collections::BTreeMap;

use crate::command::{current_platform, platform_label};
use crate::i18n::I18n;
use crate::metadata::{CommandMetadata, Registry};

pub fn print_top(registry: &Registry, i18n: &I18n) {
    let platform = current_platform();

    println!("{}", i18n.text("help.usage"));
    println!("  scv <command> [arguments]");
    println!("  scv <command> --help");
    println!("  scv --help");
    println!();
    println!("{}", i18n.text("help.available_commands"));

    for (category, commands) in
        grouped_by_category(registry.iter().filter(|command| command.supports(platform)))
    {
        println!();
        println!("{category}:");
        for command in commands {
            println!(
                "  {:<20} {}",
                command.name,
                i18n.command_description(command)
            );
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

pub fn print_command(command: &CommandMetadata, i18n: &I18n) {
    println!("{}", i18n.text("help.usage"));
    println!("  {}", command.usage);
    println!();
    println!("{}", i18n.text("help.description"));
    println!("  {}", i18n.command_description(command));
    println!();
    println!("{}", i18n.text("help.safety"));
    println!(
        "  {:<12}: {}",
        i18n.text("help.risk"),
        command.risk.as_str()
    );
    println!(
        "  {:<12}: {}",
        i18n.text("help.network"),
        if command.network {
            i18n.text("help.network_required")
        } else {
            i18n.text("help.no")
        }
    );
    println!(
        "  {:<12}: {}",
        i18n.text("help.dry_run"),
        if command.supports_dry_run {
            i18n.text("help.supported")
        } else {
            i18n.text("help.not_supported")
        }
    );
    println!("  {}", i18n.text("help.effects"));
    for (index, effect) in command.effects.iter().enumerate() {
        println!("    - {}", i18n.effect(command, index, effect));
    }

    if !command.notes.is_empty() {
        println!();
        println!("{}", i18n.text("help.notes"));
        for (index, note) in command.notes.iter().enumerate() {
            println!("  - {}", i18n.note(command, index, note));
        }
    }

    if !command.arguments.is_empty() {
        println!();
        println!("{}", i18n.text("help.arguments"));
        for argument in &command.arguments {
            let default = default_suffix(
                argument
                    .default
                    .as_deref()
                    .map(|value| i18n.default_value(command, &argument.name, value)),
                i18n,
            );
            println!(
                "  {:<34} {}{}",
                argument.name,
                i18n.argument_description(command, argument),
                default
            );
        }
    }

    println!();
    println!("{}", i18n.text("help.options"));
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
        let key = option
            .long
            .as_deref()
            .or(option.short.as_deref())
            .unwrap_or("");
        let default = default_suffix(
            option
                .default
                .as_deref()
                .map(|value| i18n.default_value(command, key, value)),
            i18n,
        );
        println!(
            "  {label:<34} {}{}",
            i18n.option_description(command, option),
            default
        );
    }
    println!("  {:<34} {}", "-h, --help", i18n.text("help.show_help"));

    if !command.examples.is_empty() {
        println!();
        println!("{}", i18n.text("help.examples"));
        for example in &command.examples {
            println!("  {}", example.command);
        }
    }
}

fn default_suffix(value: Option<&str>, i18n: &I18n) -> String {
    value
        .map(|value| format!(" ({})", i18n.format("help.default", &[("value", value)])))
        .unwrap_or_default()
}

pub fn print_info(command: &CommandMetadata, i18n: &I18n) {
    print_command(command, i18n);
    if command.implementations.is_empty() {
        println!();
        println!("{}", i18n.text("help.runtime"));
        println!("  {}", command.runtime);
        println!();
        println!("{}", i18n.text("help.platforms"));
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
            println!("{}", i18n.text("help.entry"));
            println!("  {entry}");
        }
    } else {
        println!();
        println!("{}", i18n.text("help.implementations"));
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
