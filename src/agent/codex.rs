use std::process::{Command, Stdio};

use super::{AgentAdapter, GenerateRequest};

pub struct CodexAdapter;

impl AgentAdapter for CodexAdapter {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn executable(&self) -> &'static str {
        "codex"
    }

    fn supports_agent_options(&self) -> bool {
        true
    }

    fn generate(&self, request: &GenerateRequest<'_>) -> Result<(), String> {
        let mut command = build_command(request);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let mut child = command.spawn().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "agent 'codex' is not installed or is not available on PATH".to_string()
            } else {
                format!("could not start agent 'codex': {error}")
            }
        })?;
        {
            use std::io::Write;
            let stdin = child
                .stdin
                .as_mut()
                .ok_or_else(|| "could not open stdin for agent 'codex'".to_string())?;
            stdin
                .write_all(request.prompt.as_bytes())
                .map_err(|error| format!("could not send request to agent 'codex': {error}"))?;
        }
        let status = child
            .wait()
            .map_err(|error| format!("could not wait for agent 'codex': {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("agent 'codex' failed with status {status}"))
        }
    }
}

fn build_command(request: &GenerateRequest<'_>) -> Command {
    let mut command = Command::new("codex");
    command
        .arg("exec")
        .arg("--cd")
        .arg(request.workspace)
        .arg("--sandbox")
        .arg("workspace-write")
        .arg("--ephemeral")
        .arg("--ignore-user-config")
        .arg("--ignore-rules")
        .arg("--config")
        .arg("project_doc_max_bytes=0")
        .arg("--skip-git-repo-check");
    if let Some(model) = request.model {
        command.arg("--model").arg(model);
    }
    if let Some(effort) = request.effort {
        command
            .arg("--config")
            .arg(format!("model_reasoning_effort=\"{}\"", effort.as_str()));
    }
    for (key, value) in request.options {
        command.arg("--config").arg(format!("{key}={value}"));
    }
    command.arg("-");
    command
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::build_command;
    use crate::agent::{Effort, GenerateRequest};

    #[test]
    fn translates_portable_settings_to_codex_flags() {
        let options = BTreeMap::from([("model_verbosity_level".to_string(), "low".to_string())]);
        let request = GenerateRequest {
            workspace: Path::new("workspace"),
            prompt: "request",
            model: Some("model-name"),
            effort: Some(Effort::High),
            options: &options,
        };
        let command = build_command(&request);
        let args = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--model", "model-name"])
        );
        assert!(args.contains(&"model_reasoning_effort=\"high\"".to_string()));
        assert!(args.contains(&"model_verbosity_level=low".to_string()));
        assert!(args.contains(&"--ignore-user-config".to_string()));
        assert!(args.contains(&"--ignore-rules".to_string()));
        assert!(args.contains(&"project_doc_max_bytes=0".to_string()));
    }
}
