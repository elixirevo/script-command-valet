use std::process::{Command, Stdio};

use super::{AgentAdapter, GenerateRequest};

pub struct ClaudeAdapter;

impl AgentAdapter for ClaudeAdapter {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn executable(&self) -> &'static str {
        "claude"
    }

    fn supports_agent_options(&self) -> bool {
        false
    }

    fn generate(&self, request: &GenerateRequest<'_>) -> Result<(), String> {
        if !request.options.is_empty() {
            return Err(
                "agent 'claude' does not expose generic --agent-option translation; use --model or --effort"
                    .to_string(),
            );
        }
        let mut command = build_command(request);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "agent 'claude' is not installed or is not available on PATH".to_string()
            } else {
                format!("could not start agent 'claude': {error}")
            }
        })?;
        {
            use std::io::Write;
            let stdin = child
                .stdin
                .as_mut()
                .ok_or_else(|| "could not open stdin for agent 'claude'".to_string())?;
            stdin
                .write_all(request.prompt.as_bytes())
                .map_err(|error| format!("could not send request to agent 'claude': {error}"))?;
        }
        let status = child
            .wait()
            .map_err(|error| format!("could not wait for agent 'claude': {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("agent 'claude' failed with status {status}"))
        }
    }
}

fn build_command(request: &GenerateRequest<'_>) -> Command {
    let mut command = Command::new("claude");
    command
        .current_dir(request.workspace)
        .arg("--print")
        .arg("--permission-mode")
        .arg("acceptEdits")
        .arg("--tools")
        .arg("Read,Write,Edit")
        .arg("--safe-mode")
        .arg("--no-session-persistence");
    if let Some(model) = request.model {
        command.arg("--model").arg(model);
    }
    if let Some(effort) = request.effort {
        command.arg("--effort").arg(effort.as_str());
    }
    command
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::build_command;
    use crate::agent::{Effort, GenerateRequest};

    #[test]
    fn translates_portable_settings_to_claude_flags() {
        let options = BTreeMap::new();
        let request = GenerateRequest {
            workspace: Path::new("workspace"),
            prompt: "request",
            model: Some("sonnet"),
            effort: Some(Effort::High),
            options: &options,
        };
        let command = build_command(&request);
        let args = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.windows(2).any(|pair| pair == ["--model", "sonnet"]));
        assert!(args.windows(2).any(|pair| pair == ["--effort", "high"]));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--tools", "Read,Write,Edit"])
        );
        assert!(args.contains(&"--safe-mode".to_string()));
        assert!(!args.contains(&"request".to_string()));
    }
}
