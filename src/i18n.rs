use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::metadata::{ArgumentMetadata, CommandMetadata, NoteMetadata, OptionMetadata};

const ENGLISH_CATALOG: &str = include_str!("../assets/locales/en.toml");
const KOREAN_CATALOG: &str = include_str!("../assets/locales/ko.toml");

static ENGLISH: OnceLock<Result<Catalog, String>> = OnceLock::new();
static KOREAN: OnceLock<Result<Catalog, String>> = OnceLock::new();

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Locale {
    #[default]
    En,
    Ko,
}

impl Locale {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "en" | "en-US" | "en_US" => Ok(Self::En),
            "ko" | "ko-KR" | "ko_KR" => Ok(Self::Ko),
            _ => Err(format!(
                "unsupported locale '{value}'; supported locales: en, ko"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ko => "ko",
        }
    }
}

pub struct I18n {
    locale: Locale,
    english: &'static Catalog,
    selected: &'static Catalog,
}

impl I18n {
    pub fn new(locale: Locale) -> Result<Self, String> {
        let english = load_catalog(&ENGLISH, ENGLISH_CATALOG, "en")?;
        let selected = match locale {
            Locale::En => english,
            Locale::Ko => load_catalog(&KOREAN, KOREAN_CATALOG, "ko")?,
        };
        for key in english.messages.keys() {
            if !selected.messages.contains_key(key) {
                return Err(format!(
                    "embedded '{}' locale is missing message key '{key}'",
                    locale.as_str()
                ));
            }
        }
        Ok(Self {
            locale,
            english,
            selected,
        })
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }

    pub fn text<'a>(&'a self, key: &'a str) -> &'a str {
        self.selected
            .messages
            .get(key)
            .or_else(|| self.english.messages.get(key))
            .map(String::as_str)
            .unwrap_or(key)
    }

    pub fn format(&self, key: &str, arguments: &[(&str, &str)]) -> String {
        let mut message = self.text(key).to_string();
        for (name, value) in arguments {
            message = message.replace(&format!("{{{name}}}"), value);
        }
        message
    }

    pub fn command_description<'a>(&'a self, command: &'a CommandMetadata) -> &'a str {
        self.builtin(command)
            .and_then(|messages| messages.description.as_deref())
            .unwrap_or(&command.description)
    }

    pub fn effect<'a>(
        &'a self,
        command: &'a CommandMetadata,
        index: usize,
        fallback: &'a str,
    ) -> &'a str {
        self.builtin(command)
            .and_then(|messages| messages.effects.get(index))
            .map(String::as_str)
            .unwrap_or(fallback)
    }

    pub fn argument_description<'a>(
        &'a self,
        command: &'a CommandMetadata,
        argument: &'a ArgumentMetadata,
    ) -> &'a str {
        self.builtin(command)
            .and_then(|messages| messages.arguments.get(&argument.name))
            .map(String::as_str)
            .unwrap_or(&argument.description)
    }

    pub fn option_description<'a>(
        &'a self,
        command: &'a CommandMetadata,
        option: &'a OptionMetadata,
    ) -> &'a str {
        let key = option.long.as_deref().or(option.short.as_deref());
        key.and_then(|key| {
            self.builtin(command)
                .and_then(|messages| messages.options.get(key))
        })
        .map(String::as_str)
        .unwrap_or(&option.description)
    }

    pub fn note<'a>(
        &'a self,
        command: &'a CommandMetadata,
        index: usize,
        note: &'a NoteMetadata,
    ) -> &'a str {
        self.builtin(command)
            .and_then(|messages| messages.notes.get(index))
            .map(String::as_str)
            .unwrap_or(&note.text)
    }

    pub fn default_value<'a>(
        &'a self,
        command: &'a CommandMetadata,
        key: &str,
        fallback: &'a str,
    ) -> &'a str {
        self.builtin(command)
            .and_then(|messages| messages.defaults.get(key))
            .map(String::as_str)
            .unwrap_or(fallback)
    }

    fn builtin(&self, command: &CommandMetadata) -> Option<&BuiltinMessages> {
        command
            .builtin
            .then(|| self.selected.builtins.get(&command.name))
            .flatten()
    }
}

fn load_catalog(
    cell: &'static OnceLock<Result<Catalog, String>>,
    source: &'static str,
    locale: &'static str,
) -> Result<&'static Catalog, String> {
    cell.get_or_init(|| {
        toml::from_str(source)
            .map_err(|error| format!("invalid embedded '{locale}' locale: {error}"))
    })
    .as_ref()
    .map_err(Clone::clone)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    messages: BTreeMap<String, String>,
    #[serde(default)]
    builtins: BTreeMap<String, BuiltinMessages>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuiltinMessages {
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    effects: Vec<String>,
    #[serde(default)]
    arguments: BTreeMap<String, String>,
    #[serde(default)]
    options: BTreeMap<String, String>,
    #[serde(default)]
    notes: Vec<String>,
    #[serde(default)]
    defaults: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::{I18n, Locale};
    use crate::metadata::Registry;

    #[test]
    fn embeds_complete_english_and_korean_catalogs() {
        let english = I18n::new(Locale::En).expect("English should load");
        let korean = I18n::new(Locale::Ko).expect("Korean should load");
        assert_eq!(english.text("help.usage"), "Usage:");
        assert_eq!(korean.text("help.usage"), "사용법:");
    }

    #[test]
    fn localizes_builtin_text_without_changing_canonical_metadata() {
        let registry = Registry::builtins().expect("builtins should load");
        let add = registry.get("add").expect("add should exist");
        let korean = I18n::new(Locale::Ko).expect("Korean should load");
        assert_eq!(
            add.description,
            "Register an external script or binary with SCV."
        );
        assert_eq!(
            korean.command_description(add),
            "외부 스크립트나 바이너리를 SCV에 등록합니다."
        );
    }

    #[test]
    fn korean_catalog_covers_every_builtin_metadata_field() {
        let registry = Registry::builtins().expect("builtins should load");
        let korean = I18n::new(Locale::Ko).expect("Korean should load");

        for command in registry.iter() {
            let messages = korean
                .selected
                .builtins
                .get(&command.name)
                .unwrap_or_else(|| {
                    panic!("missing Korean builtin '{}': description", command.name)
                });
            assert!(
                messages.description.is_some(),
                "missing Korean builtin '{}': description",
                command.name
            );
            assert_eq!(
                messages.effects.len(),
                command.effects.len(),
                "Korean effect count differs for '{}'",
                command.name
            );
            assert_eq!(
                messages.notes.len(),
                command.notes.len(),
                "Korean note count differs for '{}'",
                command.name
            );
            for argument in &command.arguments {
                assert!(
                    messages.arguments.contains_key(&argument.name),
                    "missing Korean argument '{}.{}'",
                    command.name,
                    argument.name
                );
                if argument.default.is_some() {
                    assert!(
                        messages.defaults.contains_key(&argument.name),
                        "missing Korean default '{}.{}'",
                        command.name,
                        argument.name
                    );
                }
            }
            for option in &command.options {
                let key = option
                    .long
                    .as_deref()
                    .or(option.short.as_deref())
                    .expect("validated option should have a name");
                assert!(
                    messages.options.contains_key(key),
                    "missing Korean option '{}.{key}'",
                    command.name
                );
                if option.default.is_some() {
                    assert!(
                        messages.defaults.contains_key(key),
                        "missing Korean default '{}.{key}'",
                        command.name
                    );
                }
            }
        }
    }
}
