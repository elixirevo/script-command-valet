use std::process::Command;

use super::{AgentAdapter, Effort, GenerateRequest, TokenUsage, UsageFormat, run_quietly};

pub struct AgyAdapter;

impl AgentAdapter for AgyAdapter {
    fn name(&self) -> &'static str {
        "agy"
    }

    fn supports_agent_options(&self) -> bool {
        false
    }

    fn validate_effort(&self, effort: Option<Effort>) -> Result<(), String> {
        validate_effort(effort)
    }

    fn generate(&self, request: &GenerateRequest<'_>) -> Result<Option<TokenUsage>, String> {
        validate_request(request)?;
        run_quietly(
            &mut build_command(request),
            self.name(),
            None,
            UsageFormat::Agy,
        )
    }
}

fn validate_request(request: &GenerateRequest<'_>) -> Result<(), String> {
    if !request.options.is_empty() {
        return Err(
            "agent 'agy' does not expose generic --agent-option translation; use --model or --effort"
                .to_string(),
        );
    }
    validate_effort(request.effort)
}

fn validate_effort(effort: Option<Effort>) -> Result<(), String> {
    if matches!(effort, Some(Effort::XHigh | Effort::Max)) {
        return Err(
            "agent 'agy' supports effort low, medium, or high; xhigh and max are unavailable"
                .to_string(),
        );
    }
    Ok(())
}

fn build_command(request: &GenerateRequest<'_>) -> Command {
    let mut command = Command::new("agy");
    command
        .current_dir(request.workspace)
        .arg("--print")
        .args(["--output-format", "stream-json"])
        .arg("--mode")
        .arg("accept-edits")
        .arg("--sandbox")
        .arg("--disable-slash-commands")
        .arg("--print-timeout")
        .arg("15m");
    if let Some(model) = request.model {
        command.arg("--model").arg(model);
    }
    if let Some(effort) = request.effort {
        command.arg("--effort").arg(effort.as_str());
    }
    command.arg(request.prompt);
    command
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::{build_command, validate_request};
    use crate::agent::{Effort, GenerateRequest};

    #[test]
    fn translates_portable_settings_to_agy_flags() {
        let options = BTreeMap::new();
        let request = GenerateRequest {
            workspace: Path::new("workspace"),
            prompt: "request",
            model: Some("model-name"),
            effort: Some(Effort::High),
            options: &options,
        };
        validate_request(&request).expect("request should be supported");
        let command = build_command(&request);
        let args = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--model", "model-name"])
        );
        assert!(args.windows(2).any(|pair| pair == ["--effort", "high"]));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--mode", "accept-edits"])
        );
        assert!(args.contains(&"--sandbox".to_string()));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--output-format", "stream-json"])
        );
        assert!(args.contains(&"--disable-slash-commands".to_string()));
        assert!(!args.contains(&"--dangerously-skip-permissions".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("request"));
        assert_eq!(command.get_current_dir(), Some(Path::new("workspace")));
    }

    #[test]
    fn rejects_effort_levels_unsupported_by_agy() {
        let options = BTreeMap::new();
        let request = GenerateRequest {
            workspace: Path::new("workspace"),
            prompt: "request",
            model: None,
            effort: Some(Effort::XHigh),
            options: &options,
        };
        assert!(validate_request(&request).is_err());
    }
}
