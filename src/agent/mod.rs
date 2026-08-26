mod claude;
mod codex;

use std::collections::BTreeMap;
use std::path::Path;

pub struct GenerateRequest<'a> {
    pub workspace: &'a Path,
    pub prompt: &'a str,
    pub model: Option<&'a str>,
    pub effort: Option<Effort>,
    pub options: &'a BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effort {
    Low,
    Medium,
    High,
    XHigh,
    Max,
}

impl Effort {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "xhigh" => Ok(Self::XHigh),
            "max" => Ok(Self::Max),
            _ => Err(format!(
                "invalid effort '{value}'; expected low, medium, high, xhigh, or max"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
        }
    }
}

pub trait AgentAdapter {
    fn name(&self) -> &'static str;
    fn executable(&self) -> &'static str;
    fn supports_agent_options(&self) -> bool;
    fn generate(&self, request: &GenerateRequest<'_>) -> Result<(), String>;
}

pub fn adapter(name: &str) -> Result<Box<dyn AgentAdapter>, String> {
    match name {
        "codex" => Ok(Box::new(codex::CodexAdapter)),
        "claude" => Ok(Box::new(claude::ClaudeAdapter)),
        _ => Err(format!(
            "unsupported agent '{name}'; supported agents: codex, claude"
        )),
    }
}

pub fn validate_option_key(key: &str) -> Result<(), String> {
    if key.is_empty()
        || !key.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
    {
        return Err(format!("invalid agent option key '{key}'"));
    }
    let normalized = key.to_ascii_lowercase();
    if [
        "approval",
        "approve",
        "danger",
        "permission",
        "sandbox",
        "network",
        "web_search",
    ]
    .iter()
    .any(|reserved| normalized.contains(reserved))
        || matches!(normalized.as_str(), "model" | "model_reasoning_effort")
    {
        return Err(format!(
            "agent option '{key}' is reserved by SCV's model or security boundary"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Effort, validate_option_key};

    #[test]
    fn validates_portable_effort_levels() {
        assert_eq!(Effort::parse("xhigh"), Ok(Effort::XHigh));
        assert!(Effort::parse("auto").is_err());
    }

    #[test]
    fn keeps_agent_options_out_of_the_security_boundary() {
        assert!(validate_option_key("model_verbosity").is_ok());
        assert!(validate_option_key("sandbox_mode").is_err());
        assert!(validate_option_key("temperature").is_ok());
    }
}
