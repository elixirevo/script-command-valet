use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, Write};

use crate::activation;
use crate::agent::{self, Effort, GenerateRequest};
use crate::command::platform_label;
use crate::config::Settings;
use crate::generation::{self, GenerationMode, GenerationWorkspace};
use crate::i18n::{I18n, Locale};
use crate::input;
use crate::metadata::{Registry, valid_name};
use crate::package;
use crate::paths::AppPaths;
use crate::progress::GenerationProgress;
use crate::source;

#[derive(Default)]
struct CreateOptions {
    description: Option<String>,
    name: Option<String>,
    locale: Option<Locale>,
    agent: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    agent_options: BTreeMap<String, String>,
    force: bool,
    yes: bool,
    no_input: bool,
}

pub fn run(
    arguments: &[OsString],
    paths: &AppPaths,
    _registry: &Registry,
    i18n: &I18n,
) -> Result<i32, String> {
    let registry = Registry::load_source(paths).map_err(|error| format!("create: {error}"))?;
    let options = parse(arguments)?;
    let description = options
        .description
        .as_deref()
        .ok_or_else(|| usage_error("missing required description"))?;
    if description.trim().is_empty() {
        return Err(usage_error("description cannot be empty"));
    }
    if description.len() > 32 * 1024 {
        return Err(usage_error("description exceeds the 32768 byte limit"));
    }
    if let Some(name) = options.name.as_deref() {
        if !valid_name(name) || name == "help" {
            return Err(usage_error(&format!("invalid command name '{name}'")));
        }
        if let Some(existing) = registry.get(name) {
            if existing.builtin {
                return Err(format!(
                    "create: '{name}' is a built-in command and cannot be replaced"
                ));
            }
            if !options.force {
                return Err(format!(
                    "create: command '{name}' already exists. Use --force to replace it"
                ));
            }
        }
    }

    let interactive = input::is_enabled(options.no_input);
    if !options.yes && !interactive {
        return Err(
            "create: --yes is required when input is disabled; no agent was started".to_string(),
        );
    }

    let settings = Settings::load(paths)?;
    let output_locale = options.locale.unwrap_or(settings.ui.locale);
    let resolved = settings.resolve_create(
        options.agent,
        options.model,
        options.effort,
        options.agent_options,
    );
    let adapter = agent::adapter(&resolved.agent)?;
    if !resolved.options.is_empty() && !adapter.supports_agent_options() {
        return Err(format!(
            "create: agent '{}' does not support generic agent options",
            adapter.name()
        ));
    }
    for key in resolved.options.keys() {
        agent::validate_option_key(key).map_err(|error| format!("create: {error}"))?;
    }
    let effort = resolved
        .effort
        .as_deref()
        .map(Effort::parse)
        .transpose()
        .map_err(|error| format!("create: {error}"))?;
    adapter
        .validate_effort(effort)
        .map_err(|error| format!("create: {error}"))?;

    let started = std::time::Instant::now();
    let preparing = GenerationProgress::start(1, i18n.text("generation.preparing"), i18n);
    let workspace = GenerationWorkspace::create(GenerationMode::Persistent)?;
    let reserved_names = registry
        .names()
        .filter(|name| !(options.force && options.name.as_deref() == Some(*name)))
        .collect::<Vec<_>>();
    let prompt = generation::build_prompt(
        description,
        options.name.as_deref(),
        output_locale,
        GenerationMode::Persistent,
        reserved_names,
    );
    preparing.complete();
    let generating = GenerationProgress::start(
        2,
        &i18n.format("generation.generating", &[("agent", adapter.name())]),
        i18n,
    );
    let usage = adapter
        .generate(&GenerateRequest {
            workspace: workspace.root(),
            prompt: &prompt,
            model: resolved.model.as_deref(),
            effort,
            options: &resolved.options,
        })
        .map_err(|error| format!("create: {error}"))?;

    generating.complete();

    let validating = GenerationProgress::start(3, i18n.text("generation.validating"), i18n);
    let generated = workspace.generated_package()?;
    let validated = package::validate_generated(&generated)
        .map_err(|error| format!("create: validation failed: {error}"))?;
    if let Some(name) = options.name.as_deref()
        && validated.metadata.name != name
    {
        return Err(format!(
            "create: validation failed: requested name '{name}' but agent generated '{}'",
            validated.metadata.name
        ));
    }
    if let Some(existing) = registry.get(&validated.metadata.name) {
        if existing.builtin {
            return Err(format!(
                "create: generated name '{}' is reserved by a built-in command",
                validated.metadata.name
            ));
        }
        if !options.force {
            return Err(format!(
                "create: command '{}' already exists. Use --name with --force to replace it",
                validated.metadata.name
            ));
        }
    }

    validating.complete();
    GenerationProgress::summary(started.elapsed(), usage, i18n);
    GenerationProgress::approval(i18n.text("generation.install_approval"));
    print_preview(&validated, i18n);
    if !options.yes && !confirm(i18n)? {
        println!("{}", i18n.text("create.cancelled"));
        return Ok(0);
    }
    source::ensure_initialized(paths).map_err(|error| format!("create: {error}"))?;
    package::install_validated(&validated, paths, &registry, options.force)
        .map_err(|error| format!("create: {error}"))?;
    let applied = activation::apply(paths).map_err(|error| {
        format!(
            "create: source was updated but activation failed; the previous activation remains active: {error}"
        )
    })?;
    println!();
    println!("{}", i18n.text("create.installed"));
    println!("  scv {}", validated.metadata.name);
    println!("  activation : {}", applied.activation);
    Ok(0)
}

fn parse(arguments: &[OsString]) -> Result<CreateOptions, String> {
    let mut options = CreateOptions::default();
    let mut index = 0;
    while index < arguments.len() {
        let argument = arguments[index]
            .to_str()
            .ok_or_else(|| "create: arguments must be valid UTF-8".to_string())?;
        match argument {
            "-n" | "--name" => set_option(
                &mut options.name,
                argument,
                value(arguments, &mut index, argument)?,
            )?,
            "--locale" => {
                let locale = value(arguments, &mut index, argument)?;
                let locale = Locale::parse(&locale).map_err(|error| usage_error(&error))?;
                set_option(&mut options.locale, argument, locale)?;
            }
            "-a" | "--agent" => set_option(
                &mut options.agent,
                argument,
                value(arguments, &mut index, argument)?,
            )?,
            "-m" | "--model" => set_option(
                &mut options.model,
                argument,
                value(arguments, &mut index, argument)?,
            )?,
            "-e" | "--effort" => set_option(
                &mut options.effort,
                argument,
                value(arguments, &mut index, argument)?,
            )?,
            "--agent-option" => {
                let raw = value(arguments, &mut index, argument)?;
                let (key, value) = raw
                    .split_once('=')
                    .ok_or_else(|| usage_error("--agent-option expects <key>=<value>"))?;
                agent::validate_option_key(key).map_err(|error| usage_error(&error))?;
                if value.is_empty() {
                    return Err(usage_error("--agent-option value cannot be empty"));
                }
                if options
                    .agent_options
                    .insert(key.to_string(), value.to_string())
                    .is_some()
                {
                    return Err(usage_error(&format!("duplicate agent option '{key}'")));
                }
            }
            "-f" | "--force" => set_flag(&mut options.force, argument)?,
            "-y" | "--yes" => set_flag(&mut options.yes, argument)?,
            "--no-input" => set_flag(&mut options.no_input, argument)?,
            value if value.starts_with('-') => {
                return Err(usage_error(&format!("unknown option '{value}'")));
            }
            _ => {
                if options.description.is_some() {
                    return Err(usage_error(&format!("unexpected argument '{argument}'")));
                }
                options.description = Some(argument.to_string());
            }
        }
        index += 1;
    }
    Ok(options)
}

fn print_preview(package: &package::ValidatedPackage, i18n: &I18n) {
    println!();
    println!("{}", i18n.text("create.preview"));
    println!("  name       : {}", package.metadata.name);
    println!("  description: {}", package.metadata.description);
    println!("  category   : {}", package.metadata.category);
    println!("  usage      : {}", package.metadata.usage);
    println!("  risk       : {}", package.metadata.risk.as_str());
    println!("  network    : {}", package.metadata.network);
    println!("  effects:");
    for effect in &package.metadata.effects {
        println!("    - {effect}");
    }
    println!("  implementations:");
    for implementation in &package.metadata.implementations {
        let platforms = implementation
            .platforms
            .iter()
            .map(|platform| platform_label(platform))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "    - {platforms}: {} ({})",
            implementation.entry, implementation.runtime
        );
    }
    println!("  files:");
    for file in &package.files {
        println!("    - {file}");
    }
    println!("  validation:");
    for check in &package.checks {
        let marker = if check.contains("skipped") {
            "-"
        } else {
            "✓"
        };
        println!("    {marker} {check}");
    }
}

fn confirm(i18n: &I18n) -> Result<bool, String> {
    print!("{}", i18n.text("create.confirm"));
    io::stdout()
        .flush()
        .map_err(|error| format!("create: could not write prompt: {error}"))?;
    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .map_err(|error| format!("create: could not read confirmation: {error}"))?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
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

fn usage_error(message: &str) -> String {
    format!("create: {message}. Try 'scv create --help' for more information")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::parse;
    use crate::i18n::Locale;

    #[test]
    fn parses_a_fully_specified_noninteractive_request() {
        let arguments = [
            "make a tool",
            "--agent",
            "codex",
            "--model",
            "model",
            "--locale",
            "ko",
            "--effort",
            "high",
            "--yes",
            "--no-input",
        ]
        .map(OsString::from);
        let options = parse(&arguments).expect("arguments should parse");
        assert_eq!(options.description.as_deref(), Some("make a tool"));
        assert_eq!(options.agent.as_deref(), Some("codex"));
        assert_eq!(options.locale, Some(Locale::Ko));
        assert!(options.yes);
        assert!(options.no_input);
    }

    #[test]
    fn rejects_duplicate_provider_options() {
        let arguments = [
            "make a tool",
            "--agent-option",
            "temperature=1",
            "--agent-option",
            "temperature=2",
        ]
        .map(OsString::from);
        assert!(parse(&arguments).is_err());
    }

    #[test]
    fn rejects_an_unsupported_generation_locale() {
        let arguments = ["make a tool", "--locale", "fr"].map(OsString::from);
        let error = parse(&arguments).err().expect("locale should be rejected");
        assert!(error.contains("supported locales: en, ko"));
    }
}
