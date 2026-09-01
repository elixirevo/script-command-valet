use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;

use crate::i18n::Locale;
use crate::paths::AppPaths;
use crate::storage;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default)]
    pub ui: UiSettings,
    #[serde(default)]
    pub create: CreateSettings,
    #[serde(default)]
    pub agents: BTreeMap<String, AgentSettings>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiSettings {
    #[serde(default)]
    pub locale: Locale,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct CreateRequestSettings {
    pub agent: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub options: BTreeMap<String, String>,
}

impl Settings {
    pub fn load(paths: &AppPaths) -> Result<Self, String> {
        let path = paths.config_path();
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(format!(
                    "could not read config '{}': {error}",
                    path.display()
                ));
            }
        };
        toml::from_str(&contents)
            .map_err(|error| format!("invalid config TOML '{}': {error}", path.display()))
    }

    pub fn save(&self, paths: &AppPaths) -> Result<(), String> {
        paths.ensure_config_dir()?;
        let contents = toml::to_string_pretty(self)
            .map_err(|error| format!("could not serialize config: {error}"))?;
        storage::write_atomic(&paths.config_path(), contents.as_bytes())
    }

    pub fn resolve_create(
        &self,
        agent: Option<String>,
        model: Option<String>,
        effort: Option<String>,
        options: BTreeMap<String, String>,
    ) -> CreateRequestSettings {
        let agent = agent
            .or_else(|| self.create.agent.clone())
            .unwrap_or_else(|| "codex".to_string());
        let agent_settings = self.agents.get(&agent);
        let model = model
            .or_else(|| agent_settings.and_then(|settings| settings.model.clone()))
            .or_else(|| self.create.model.clone());
        let effort = effort
            .or_else(|| agent_settings.and_then(|settings| settings.effort.clone()))
            .or_else(|| self.create.effort.clone());
        let mut resolved_options = self.create.options.clone();
        if let Some(settings) = agent_settings {
            resolved_options.extend(settings.options.clone());
        }
        resolved_options.extend(options);
        CreateRequestSettings {
            agent,
            model,
            effort,
            options: resolved_options,
        }
    }

    pub fn set(&mut self, key: &str, value: String) -> Result<(), String> {
        match key {
            "ui.locale" => self.ui.locale = Locale::parse(&value)?,
            "agent" | "create.agent" => self.create.agent = Some(value),
            "create.model" => self.create.model = Some(value),
            "create.effort" => self.create.effort = Some(value),
            "model" | "effort" => {
                let agent = self
                    .create
                    .agent
                    .clone()
                    .unwrap_or_else(|| "codex".to_string());
                let settings = self.agents.entry(agent).or_default();
                if key == "model" {
                    settings.model = Some(value);
                } else {
                    settings.effort = Some(value);
                }
            }
            _ => {
                let parts = key.split('.').collect::<Vec<_>>();
                if let ["agents", agent, field] = parts.as_slice() {
                    let settings = self.agents.entry((*agent).to_string()).or_default();
                    match *field {
                        "model" => settings.model = Some(value),
                        "effort" => settings.effort = Some(value),
                        _ => return Err(format!("unsupported config key '{key}'")),
                    }
                } else if let ["agents", agent, "options", option] = parts.as_slice() {
                    self.agents
                        .entry((*agent).to_string())
                        .or_default()
                        .options
                        .insert((*option).to_string(), value);
                } else {
                    return Err(format!("unsupported config key '{key}'"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{AgentSettings, Settings};

    #[test]
    fn resolves_create_settings_in_precedence_order() {
        let mut settings = Settings::default();
        settings.create.agent = Some("codex".to_string());
        settings.create.model = Some("global".to_string());
        settings.create.effort = Some("low".to_string());
        settings.agents.insert(
            "codex".to_string(),
            AgentSettings {
                model: Some("agent-model".to_string()),
                effort: Some("medium".to_string()),
                options: BTreeMap::from([("one".to_string(), "agent".to_string())]),
            },
        );
        let resolved = settings.resolve_create(
            None,
            Some("cli-model".to_string()),
            None,
            BTreeMap::from([("one".to_string(), "cli".to_string())]),
        );
        assert_eq!(resolved.agent, "codex");
        assert_eq!(resolved.model.as_deref(), Some("cli-model"));
        assert_eq!(resolved.effort.as_deref(), Some("medium"));
        assert_eq!(resolved.options.get("one").map(String::as_str), Some("cli"));
    }
}
