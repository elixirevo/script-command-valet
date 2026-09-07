mod agy;
mod claude;
mod codex;
mod usage;

pub use usage::TokenUsage;
use usage::{UsageFormat, read_usage};

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

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
    fn supports_agent_options(&self) -> bool;
    fn validate_effort(&self, _effort: Option<Effort>) -> Result<(), String> {
        Ok(())
    }
    fn generate(&self, request: &GenerateRequest<'_>) -> Result<Option<TokenUsage>, String>;
}

/// Suppress provider transcripts while extracting only reported usage from bounded
/// structured stdout events. Stderr is discarded, including on failure.
fn run_quietly(
    command: &mut Command,
    agent: &str,
    prompt: Option<&str>,
    format: UsageFormat,
) -> Result<Option<TokenUsage>, String> {
    command
        .stdin(if prompt.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("agent '{agent}' is not installed or is not available on PATH")
        } else {
            format!("could not start agent '{agent}': {error}")
        }
    })?;
    let stdout = child
        .stdout
        .take()
        .expect("stdout was configured as a pipe");
    let reader = match std::thread::Builder::new()
        .name("scv-agent-usage".to_string())
        .spawn(move || read_usage(stdout, format))
    {
        Ok(reader) => reader,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "could not read output from agent '{agent}': {error}"
            ));
        }
    };
    if let Some(prompt) = prompt {
        // Taking and dropping stdin sends EOF even when the prompt is empty.
        let sent = child
            .stdin
            .take()
            .ok_or_else(|| format!("could not open stdin for agent '{agent}'"))
            .and_then(|mut stdin| {
                stdin
                    .write_all(prompt.as_bytes())
                    .map_err(|error| format!("could not send request to agent '{agent}': {error}"))
            });
        if let Err(error) = sent {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err(error);
        }
    }
    let status = match child.wait() {
        Ok(status) => status,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err(format!("could not wait for agent '{agent}': {error}"));
        }
    };
    let report = reader.join().ok().and_then(Result::ok).unwrap_or_default();
    if !status.success() {
        return Err(format!(
            "agent '{agent}' failed with status {status}. Check the agent's authentication and configuration"
        ));
    }
    if report.failed {
        return Err(format!("agent '{agent}' reported a failed generation"));
    }
    Ok(report.tokens)
}

pub fn adapter(name: &str) -> Result<Box<dyn AgentAdapter>, String> {
    match name {
        "codex" => Ok(Box::new(codex::CodexAdapter)),
        "claude" => Ok(Box::new(claude::ClaudeAdapter)),
        "agy" => Ok(Box::new(agy::AgyAdapter)),
        _ => Err(format!(
            "unsupported agent '{name}'; supported agents: codex, claude, agy"
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
