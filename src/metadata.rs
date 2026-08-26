use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::paths::AppPaths;

const BUILTIN_SPECS: &[&str] = &[
    include_str!("builtin/metadata/add.toml"),
    include_str!("builtin/metadata/apply.toml"),
    include_str!("builtin/metadata/config.toml"),
    include_str!("builtin/metadata/create.toml"),
    include_str!("builtin/metadata/info.toml"),
    include_str!("builtin/metadata/list.toml"),
    include_str!("builtin/metadata/paths.toml"),
    include_str!("builtin/metadata/rm.toml"),
    include_str!("builtin/metadata/rollback.toml"),
    include_str!("builtin/metadata/status.toml"),
    include_str!("builtin/metadata/sync.toml"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMetadata {
    pub name: String,
    pub category: String,
    pub description: String,
    pub usage: String,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub runtime: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implementations: Vec<ImplementationMetadata>,
    pub risk: Risk,
    pub network: bool,
    pub supports_dry_run: bool,
    pub effects: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<ArgumentMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<OptionMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<NoteMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<ExampleMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplementationMetadata {
    pub runtime: String,
    pub platforms: Vec<String>,
    pub entry: String,
}

#[derive(Debug, Clone, Copy)]
pub struct ResolvedImplementation<'a> {
    pub runtime: &'a str,
    pub entry: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Read,
    Write,
    Destructive,
}

impl Risk {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Destructive => "destructive",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgumentMetadata {
    pub name: String,
    #[serde(default)]
    pub required: bool,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMetadata {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExampleMetadata {
    pub command: String,
}

impl CommandMetadata {
    pub fn from_toml(contents: &str) -> Result<Self, String> {
        toml::from_str(contents).map_err(|error| format!("invalid metadata TOML: {error}"))
    }

    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string_pretty(self)
            .map_err(|error| format!("could not serialize metadata: {error}"))
    }

    pub fn validate(&self) -> Result<(), String> {
        if !valid_name(&self.name) {
            return Err(format!("invalid command name '{}'", self.name));
        }
        if self.name == "help" {
            return Err("command name 'help' is reserved".to_string());
        }
        if !valid_name(&self.category) {
            return Err(format!("invalid category '{}'", self.category));
        }
        if !is_nonempty_line(&self.description) {
            return Err(format!("'{}' must have a one-line description", self.name));
        }
        if !is_nonempty_line(&self.usage) {
            return Err(format!("'{}' must have a one-line usage", self.name));
        }
        if self.effects.is_empty() {
            return Err(format!("'{}' must declare at least one effect", self.name));
        }
        for effect in &self.effects {
            if !is_nonempty_line(effect) {
                return Err(format!("'{}' has an invalid effect", self.name));
            }
        }
        let declares_dry_run = self
            .options
            .iter()
            .any(|option| option.long.as_deref() == Some("--dry-run"));
        if self.supports_dry_run != declares_dry_run {
            return Err(format!(
                "'{}' must keep supports_dry_run aligned with its --dry-run option",
                self.name
            ));
        }
        self.validate_help_contract()?;

        if self.builtin {
            if self.runtime != "builtin" || self.entry.is_some() || !self.implementations.is_empty()
            {
                return Err(format!(
                    "builtin '{}' must use runtime='builtin' without entry or implementations",
                    self.name
                ));
            }
            validate_platforms(&self.name, &self.platforms)?;
        } else {
            self.validate_external_contract()?;
        }

        Ok(())
    }

    pub fn supports(&self, platform: &str) -> bool {
        self.implementation_for(platform).is_some()
            || (self.builtin && self.platforms.iter().any(|candidate| candidate == platform))
    }

    pub fn implementation_for(&self, platform: &str) -> Option<ResolvedImplementation<'_>> {
        if let Some(implementation) = self.implementations.iter().find(|implementation| {
            implementation
                .platforms
                .iter()
                .any(|value| value == platform)
        }) {
            return Some(ResolvedImplementation {
                runtime: &implementation.runtime,
                entry: &implementation.entry,
            });
        }
        None
    }

    pub fn supported_platforms(&self) -> Vec<String> {
        let mut platforms = BTreeSet::new();
        if self.builtin {
            platforms.extend(self.platforms.iter().cloned());
        } else {
            for implementation in &self.implementations {
                platforms.extend(implementation.platforms.iter().cloned());
            }
        }
        platforms.into_iter().collect()
    }

    pub fn runtime_label(&self, platform: &str) -> String {
        if self.builtin {
            return self.runtime.clone();
        }
        if let Some(implementation) = self.implementation_for(platform) {
            return implementation.runtime.to_string();
        }
        self.implementations
            .iter()
            .map(|implementation| implementation.runtime.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn validate_external_contract(&self) -> Result<(), String> {
        if !self.runtime.is_empty() || !self.platforms.is_empty() || self.entry.is_some() {
            return Err(format!(
                "external command '{}' must declare runtime, platforms, and entry only through [[implementations]]",
                self.name
            ));
        }
        if self.implementations.is_empty() {
            return Err(format!(
                "external command '{}' must declare at least one [[implementations]] entry",
                self.name
            ));
        }

        let mut assigned_platforms = BTreeSet::new();
        for implementation in &self.implementations {
            validate_runtime(&self.name, &implementation.runtime)?;
            validate_platforms(&self.name, &implementation.platforms)?;
            if !valid_entry(&implementation.entry) {
                return Err(format!(
                    "'{}' has unsafe implementation entry '{}'",
                    self.name, implementation.entry
                ));
            }
            for platform in &implementation.platforms {
                if !assigned_platforms.insert(platform) {
                    return Err(format!(
                        "'{}' declares more than one implementation for '{platform}'",
                        self.name
                    ));
                }
            }
        }
        Ok(())
    }

    fn validate_help_contract(&self) -> Result<(), String> {
        let usage_prefix = format!("scv {}", self.name);
        if !starts_with_command(&self.usage, &usage_prefix) {
            return Err(format!(
                "'{}' usage must start with '{usage_prefix}'",
                self.name
            ));
        }
        if self.arguments.len() > 2 {
            return Err(format!(
                "'{}' must keep at most two core positional arguments",
                self.name
            ));
        }
        let mut argument_names = BTreeSet::new();
        for argument in &self.arguments {
            if !is_nonempty_line(&argument.name) || !is_nonempty_line(&argument.description) {
                return Err(format!("'{}' has an invalid argument entry", self.name));
            }
            if !argument_names.insert(&argument.name) {
                return Err(format!(
                    "'{}' declares duplicate argument '{}'",
                    self.name, argument.name
                ));
            }
            if argument
                .default
                .as_deref()
                .is_some_and(|value| !is_nonempty_line(value))
            {
                return Err(format!(
                    "'{}' argument '{}' has an invalid default",
                    self.name, argument.name
                ));
            }
        }

        let mut option_names = BTreeSet::new();
        for option in &self.options {
            if option.short.is_none() && option.long.is_none() {
                return Err(format!("'{}' has an option without a name", self.name));
            }
            if !is_nonempty_line(&option.description) {
                return Err(format!("'{}' has an invalid option description", self.name));
            }
            if let Some(short) = option.short.as_deref() {
                let bytes = short.as_bytes();
                if bytes.len() != 2
                    || bytes[0] != b'-'
                    || !bytes[1].is_ascii_alphanumeric()
                    || short == "-h"
                {
                    return Err(format!(
                        "'{}' has invalid short option '{short}'",
                        self.name
                    ));
                }
                if !option_names.insert(short) {
                    return Err(format!(
                        "'{}' declares duplicate option '{short}'",
                        self.name
                    ));
                }
            }
            if let Some(long) = option.long.as_deref() {
                let valid = long == "--" || long.strip_prefix("--").is_some_and(valid_name);
                if !valid || matches!(long, "-h" | "--help") {
                    return Err(format!("'{}' has invalid long option '{long}'", self.name));
                }
                if !option_names.insert(long) {
                    return Err(format!(
                        "'{}' declares duplicate option '{long}'",
                        self.name
                    ));
                }
            }
            if option
                .value
                .as_deref()
                .is_some_and(|value| !is_nonempty_line(value))
                || option
                    .default
                    .as_deref()
                    .is_some_and(|value| !is_nonempty_line(value))
            {
                return Err(format!(
                    "'{}' has an invalid option value/default",
                    self.name
                ));
            }
        }
        for note in &self.notes {
            if !is_nonempty_line(&note.text) {
                return Err(format!("'{}' has an invalid note", self.name));
            }
        }
        for example in &self.examples {
            if !is_nonempty_line(&example.command)
                || !starts_with_command(&example.command, &usage_prefix)
            {
                return Err(format!("'{}' has an invalid example", self.name));
            }
        }
        Ok(())
    }
}

fn validate_runtime(command: &str, runtime: &str) -> Result<(), String> {
    if matches!(runtime, "bash" | "node" | "python" | "pwsh" | "binary") {
        Ok(())
    } else {
        Err(format!("'{command}' has unsupported runtime '{runtime}'"))
    }
}

fn validate_platforms(command: &str, platforms: &[String]) -> Result<(), String> {
    if platforms.is_empty() {
        return Err(format!("'{command}' must declare at least one platform"));
    }
    for platform in platforms {
        if !matches!(platform.as_str(), "windows" | "linux" | "macos") {
            return Err(format!(
                "'{command}' declares unsupported platform '{platform}'"
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Registry {
    commands: BTreeMap<String, CommandMetadata>,
}

impl Registry {
    pub fn load(paths: &AppPaths) -> Result<Self, String> {
        Self::load_from_command_dir(&paths.active_command_dir)
    }

    pub fn load_source(paths: &AppPaths) -> Result<Self, String> {
        Self::load_from_command_dir(&paths.source_command_dir)
    }

    pub fn load_from_command_dir(command_dir: &Path) -> Result<Self, String> {
        let mut registry = Self::builtins()?;
        load_packages(command_dir, &mut registry.commands)?;
        Ok(registry)
    }

    pub fn builtins() -> Result<Self, String> {
        let mut commands = BTreeMap::new();
        for spec in BUILTIN_SPECS {
            let metadata = CommandMetadata::from_toml(spec)?;
            metadata.validate()?;
            commands.insert(metadata.name.clone(), metadata);
        }
        Ok(Self { commands })
    }

    pub fn get(&self, name: &str) -> Option<&CommandMetadata> {
        self.commands.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &CommandMetadata> {
        self.commands.values()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.commands.keys().map(String::as_str)
    }
}

fn load_packages(
    command_dir: &Path,
    commands: &mut BTreeMap<String, CommandMetadata>,
) -> Result<(), String> {
    if !command_dir.is_dir() {
        return Ok(());
    }
    let entries = fs::read_dir(command_dir).map_err(|error| {
        format!(
            "could not read command directory '{}': {error}",
            command_dir.display()
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| format!("could not read command: {error}"))?;
        let package_path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "could not inspect command storage '{}': {error}",
                package_path.display()
            )
        })?;
        if file_type.is_symlink() {
            return Err(format!(
                "command package '{}' must not be a symbolic link",
                package_path.display()
            ));
        }
        if !file_type.is_dir() {
            continue;
        }
        let package_name = entry.file_name();
        let package_name = package_name.to_string_lossy();
        if package_name.starts_with('.') {
            continue;
        }
        let metadata_path = package_path.join("metadata.toml");
        let metadata = read_metadata(&metadata_path)?;
        if metadata.builtin {
            return Err(format!(
                "{}: command packages cannot declare builtin=true",
                metadata_path.display()
            ));
        }
        if metadata.implementations.is_empty() {
            return Err(format!(
                "{}: command packages must declare at least one [[implementations]] entry",
                metadata_path.display()
            ));
        }
        metadata.validate()?;
        if package_name != metadata.name {
            return Err(format!(
                "command package '{}' does not match command name '{}'",
                package_path.display(),
                metadata.name
            ));
        }
        for implementation in &metadata.implementations {
            let implementation_path = package_path.join(&implementation.entry);
            let implementation_file =
                fs::symlink_metadata(&implementation_path).map_err(|error| {
                    format!(
                        "could not inspect implementation '{}': {error}",
                        implementation_path.display()
                    )
                })?;
            if !implementation_file.is_file() || implementation_file.file_type().is_symlink() {
                return Err(format!(
                    "implementation '{}' must be a regular non-symlink file",
                    implementation_path.display()
                ));
            }
        }
        let name = metadata.name.clone();
        if commands.insert(name.clone(), metadata).is_some() {
            return Err(format!("duplicate command metadata for '{name}'"));
        }
    }
    Ok(())
}

fn read_metadata(path: &Path) -> Result<CommandMetadata, String> {
    let file = fs::symlink_metadata(path)
        .map_err(|error| format!("could not inspect metadata '{}': {error}", path.display()))?;
    if !file.is_file() || file.file_type().is_symlink() {
        return Err(format!(
            "metadata '{}' must be a regular non-symlink file",
            path.display()
        ));
    }
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("could not read metadata '{}': {error}", path.display()))?;
    CommandMetadata::from_toml(&contents).map_err(|error| format!("{}: {error}", path.display()))
}

fn is_nonempty_line(value: &str) -> bool {
    !value.trim().is_empty() && !value.contains(['\n', '\r'])
}

fn starts_with_command(value: &str, command: &str) -> bool {
    value == command
        || value
            .strip_prefix(command)
            .is_some_and(|suffix| suffix.starts_with(' '))
}

pub fn valid_name(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some(character) if character.is_ascii_alphanumeric())
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
}

pub fn valid_entry(value: &str) -> bool {
    valid_name(value) && !value.ends_with('.')
}

#[cfg(test)]
mod tests {
    use super::{CommandMetadata, valid_entry, valid_name};

    const WRITE_COMMAND_METADATA: &str = r#"
name = "repository-clone"
category = "example"
description = "Clone repositories from a remote service."
usage = "scv repository-clone <organization> [--directory <path>] [--dry-run]"
builtin = false
risk = "write"
network = true
supports_dry_run = true
effects = ["creates local directories", "downloads remote repositories"]

[[implementations]]
runtime = "bash"
platforms = ["linux", "macos"]
entry = "main.sh"

[[arguments]]
name = "organization"
required = true
description = "Remote organization name"

[[options]]
short = "-d"
long = "--directory"
value = "<path>"
description = "Clone destination"

[[options]]
long = "--dry-run"
description = "Print the plan without cloning"

[[examples]]
command = "scv repository-clone example --dry-run"
"#;

    const MULTIPLATFORM_COMMAND_METADATA: &str = r#"
name = "path-size"
category = "example"
description = "Show the size of a path."
usage = "scv path-size [path]"
builtin = false
risk = "read"
network = false
supports_dry_run = false
effects = ["reads local path metadata"]

[[implementations]]
runtime = "bash"
platforms = ["linux", "macos"]
entry = "unix.sh"

[[implementations]]
runtime = "pwsh"
platforms = ["windows"]
entry = "windows.ps1"

[[arguments]]
name = "path"
required = false
description = "Path to inspect"
"#;

    #[test]
    fn validates_names_and_entries() {
        assert!(valid_name("gh-org-clone"));
        assert!(!valid_name("-hidden"));
        assert!(!valid_name("nested/name"));
        assert!(valid_entry("tool.exe"));
        assert!(!valid_entry("../tool.exe"));
        assert!(!valid_entry("..\\tool.exe"));
        assert!(!valid_entry("nested/tool.exe"));
    }

    #[test]
    fn parses_detailed_metadata() {
        let metadata =
            CommandMetadata::from_toml(WRITE_COMMAND_METADATA).expect("metadata should parse");
        metadata.validate().expect("metadata should validate");
        let implementation = metadata
            .implementation_for("macos")
            .expect("macOS implementation should exist");
        assert_eq!(implementation.runtime, "bash");
        assert_eq!(implementation.entry, "main.sh");
        assert_eq!(metadata.arguments.len(), 1);
        assert!(metadata.options.iter().any(|option| {
            option.short.as_deref() == Some("-d") && option.long.as_deref() == Some("--directory")
        }));
        assert_eq!(metadata.risk.as_str(), "write");
        assert!(metadata.network);
        assert!(metadata.supports_dry_run);
    }

    #[test]
    fn rejects_incomplete_safety_contracts() {
        let metadata =
            CommandMetadata::from_toml(WRITE_COMMAND_METADATA).expect("metadata should parse");

        let mut missing_effects = metadata.clone();
        missing_effects.effects.clear();
        assert!(missing_effects.validate().is_err());

        let mut mismatched_dry_run = metadata;
        mismatched_dry_run.supports_dry_run = false;
        assert!(mismatched_dry_run.validate().is_err());
    }

    #[test]
    fn rejects_invalid_help_contracts() {
        let metadata =
            CommandMetadata::from_toml(WRITE_COMMAND_METADATA).expect("metadata should parse");

        let mut duplicate_option = metadata.clone();
        duplicate_option
            .options
            .push(duplicate_option.options[0].clone());
        assert!(duplicate_option.validate().is_err());

        let mut multiline_note = metadata;
        multiline_note.notes.push(super::NoteMetadata {
            text: "invalid\nnote".to_string(),
        });
        assert!(multiline_note.validate().is_err());
    }

    #[test]
    fn resolves_platform_specific_implementations() {
        let metadata = CommandMetadata::from_toml(MULTIPLATFORM_COMMAND_METADATA)
            .expect("metadata should parse");
        metadata.validate().expect("metadata should validate");

        let windows = metadata
            .implementation_for("windows")
            .expect("Windows implementation should exist");
        assert_eq!(windows.runtime, "pwsh");
        assert_eq!(windows.entry, "windows.ps1");
        assert_eq!(
            metadata.supported_platforms(),
            vec!["linux", "macos", "windows"]
        );
    }

    #[test]
    fn rejects_duplicate_platform_implementations() {
        let mut metadata = CommandMetadata::from_toml(MULTIPLATFORM_COMMAND_METADATA)
            .expect("metadata should parse");
        metadata.implementations[1]
            .platforms
            .push("macos".to_string());
        assert!(metadata.validate().is_err());
    }

    #[test]
    fn rejects_external_top_level_runtime_fields() {
        let metadata = CommandMetadata::from_toml(
            r#"
name = "old-shape"
category = "test"
description = "invalid external shape"
usage = "scv old-shape"
builtin = false
runtime = "bash"
platforms = ["linux"]
entry = "main.sh"
risk = "read"
network = false
supports_dry_run = false
effects = ["reads test input"]
"#,
        )
        .expect("metadata should parse");
        assert!(metadata.validate().is_err());
    }
}
