#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "scv generation output {} {nonce} {}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        for directory in ["agents", "fixture", "tmp", "config", "caller"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(
            root.join("fixture/metadata.toml"),
            r#"name = "quiet-command"
category = "test"
description = "Show the approved result"
usage = "scv quiet-command"
builtin = false
risk = "read"
network = false
supports_dry_run = false
effects = ["prints the approved result"]
[[implementations]]
runtime = "bash"
platforms = ["linux", "macos"]
entry = "main.sh"
"#,
        )
        .unwrap();
        fs::write(
            root.join("fixture/main.sh"),
            "#!/usr/bin/env bash\nset -euo pipefail\n# HIDDEN_SOURCE\nprintf 'PUBLIC_RESULT\\n'\npwd\nprintf '%s\\n' \"$SCV_COMMAND_PACKAGE_DIR\"\n",
        )
        .unwrap();
        let agent = r#"#!/bin/sh
set -eu
# Write more than a pipe buffer before reading stdin, including terminal codes.
i=0
while [ "$i" -lt 3000 ]; do
    printf '%s\n' '{"type":"diagnostic","message":"\u001b[31mHIDDEN_TRANSCRIPT source and tool output\u001b[0m"}'
    printf 'HIDDEN_TRANSCRIPT reasoning and diagnostics\n' >&2
    i=$((i + 1))
done
if [ "${SCV_TEST_MALFORMED_STREAM:-0}" = 1 ]; then
    printf '\033[31mHIDDEN_TRANSCRIPT malformed stdout\033[0m\n'
fi
case "$0" in
    */codex)
        while [ "$1" != "--cd" ]; do shift; done
        cd "$2"
        cat > "$SCV_TEST_ROOT/prompt"
        ;;
    */claude) cat > "$SCV_TEST_ROOT/prompt" ;;
    */agy)
        for argument in "$@"; do prompt="$argument"; done
        printf '%s' "$prompt" > "$SCV_TEST_ROOT/prompt"
        ;;
esac
pwd > "$SCV_TEST_ROOT/workspace"
if [ "${SCV_TEST_FAIL:-0}" = 1 ]; then exit 17; fi
cp -R "$SCV_TEST_ROOT/fixture" generated/quiet-command
if [ "${SCV_TEST_MISSING_USAGE:-0}" = 1 ]; then exit 0; fi
case "$0" in
    */codex)
        if [ "${SCV_TEST_RESULT_ERROR:-0}" = 1 ]; then
            printf '%s\n' '{"type":"turn.failed","error":{"message":"HIDDEN_FAILURE"}}'
        else
            printf '%s\n' '{"type":"turn.started"}'
            printf '%s\n' '{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"HIDDEN_TOOL","status":"in_progress"}}'
            printf '%s\n' '{"type":"item.updated","item":{"id":"item_1","type":"command_execution","aggregated_output":"HIDDEN_TOOL_OUTPUT","status":"in_progress"}}'
            printf '%s\n' '{"type":"item.completed","item":{"id":"item_1","type":"command_execution","status":"completed"}}'
            printf '%s\n' '{"type":"item.completed","item":{"id":"item_1","type":"command_execution","status":"completed"}}'
            printf '%s\n' '{"type":"item.completed","item":{"id":"item_2","type":"file_change","changes":[{"path":"HIDDEN_PATH","kind":"add"}],"status":"completed"}}'
            printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":12000,"cached_input_tokens":10000,"output_tokens":800}}'
        fi
        ;;
    */claude)
        if [ "${SCV_TEST_RESULT_ERROR:-0}" = 1 ]; then
            printf '%s\n' '{"type":"result","is_error":true,"result":"HIDDEN_FAILURE"}'
        else
            printf '%s\n' '{"type":"assistant","message":{"id":"message_1","content":[{"type":"tool_use","id":"tool_1","name":"Write","input":{"content":"HIDDEN_SOURCE"}}]}}'
            printf '%s\n' '{"type":"assistant","message":{"id":"message_1","content":[{"type":"tool_use","id":"tool_1","name":"Write","input":{"content":"HIDDEN_SOURCE"}}]}}'
            printf '%s\n' '{"type":"assistant","message":{"id":"message_1","content":[{"type":"tool_use","id":"tool_2","name":"Read","input":{"file_path":"HIDDEN_PATH"}}]}}'
            printf '%s\n' '{"type":"result","is_error":false,"result":"HIDDEN_RESPONSE","usage":{"input_tokens":1000,"cache_read_input_tokens":10000,"cache_creation_input_tokens":1000,"output_tokens":800}}'
        fi
        ;;
    */agy)
        if [ "${SCV_TEST_RESULT_ERROR:-0}" = 1 ]; then
            printf '%s\n' '{"event":"result","result":{"status":"ERROR","error":"HIDDEN_FAILURE"}}'
        else
            printf '%s\n' '{"event":"step_update","step_update":{"conversation_id":"conversation_1","step_index":1,"step_type":"tool","state":"ACTIVE","tool_name":"HIDDEN_TOOL"}}'
            printf '%s\n' '{"event":"step_update","step_update":{"conversation_id":"conversation_1","step_index":1,"step_type":"tool","state":"DONE","tool_info":{"output":"HIDDEN_TOOL_OUTPUT"}}}'
            printf '%s\n' '{"event":"step_update","step_update":{"conversation_id":"conversation_1","step_index":1,"step_type":"tool","state":"DONE"}}'
            printf '%s\n' '{"event":"step_update","step_update":{"conversation_id":"conversation_1","step_index":2,"step_type":"tool","state":"DONE"}}'
            printf '%s\n' '{"event":"result","result":{"status":"SUCCESS","response":"HIDDEN_RESPONSE","usage":{"input_tokens":12000,"cache_read_tokens":10000,"output_tokens":800,"thinking_tokens":300,"total_tokens":12800}}}'
        fi
        ;;
esac
"#;
        for name in ["codex", "claude", "agy"] {
            let path = root.join("agents").join(name);
            fs::write(&path, agent).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self { root }
    }

    fn command(&self, persistent: bool, agent: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_scv"));
        let mut search_paths = vec![self.root.join("agents")];
        search_paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        command
            .current_dir(self.root.join("caller"))
            .env("PATH", std::env::join_paths(search_paths).unwrap())
            .env("SCV_HOME", self.root.join("source"))
            .env("SCV_DATA_DIR", self.root.join("data"))
            .env("SCV_CONFIG_DIR", self.root.join("config"))
            .env("SCV_CACHE_DIR", self.root.join("cache"))
            .env("SCV_TEST_ROOT", &self.root)
            .env("TMPDIR", self.root.join("tmp"))
            .stdin(Stdio::null());
        if persistent {
            command.arg("create");
        }
        command.args(["Show the approved result", "--agent", agent]);
        command
    }

    fn assert_workspace_cleaned(&self) {
        let workspace = fs::read_to_string(self.root.join("workspace")).unwrap();
        assert!(!PathBuf::from(workspace.trim()).exists());
        let prompt = fs::read_to_string(self.root.join("prompt")).unwrap();
        assert!(prompt.contains("<scv-user-request>"));
        assert!(prompt.contains("Show the approved result"));
    }

    fn use_directory_size_example(&self) {
        let guide = include_str!("../assets/generation/prompts/one-shot.md");
        let (_, example) = guide.split_once("```bash\n").unwrap();
        let (source, _) = example.split_once("```").unwrap();
        fs::write(self.root.join("fixture/main.sh"), source).unwrap();
        let metadata_path = self.root.join("fixture/metadata.toml");
        let metadata = fs::read_to_string(&metadata_path)
            .unwrap()
            .replace("Show the approved result", "Show directory disk usage")
            .replace(
                "prints the approved result",
                "reads directory disk usage and prints sizes in ascending order",
            );
        fs::write(metadata_path, metadata).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn visible_output(output: &Output) -> (String, String) {
    let stdout = String::from_utf8(output.stdout.clone()).unwrap();
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    for stream in [&stdout, &stderr] {
        assert!(
            !stream.contains("HIDDEN_"),
            "provider output leaked: {stream}"
        );
        assert!(!stream.contains("<scv-user-request>"));
        assert!(!stream.contains('\r'));
        assert!(!stream.contains('\x1b'));
    }
    assert!(!stdout.contains("[1/4]"));
    (stdout, stderr)
}

#[test]
fn hides_all_provider_streams_but_preserves_progress_preview_and_approved_output() {
    for agent in ["codex", "claude", "agy"] {
        for persistent in [false, true] {
            let fixture = Fixture::new();
            let output = fixture
                .command(persistent, agent)
                .args(["--yes", "--no-input"])
                .output()
                .unwrap();
            let (stdout, stderr) = visible_output(&output);
            assert!(output.status.success(), "{agent}: {stderr}");
            let stages = stderr
                .lines()
                .filter(|line| line.starts_with('['))
                .collect::<Vec<_>>();
            assert_eq!(stages.len(), 4, "{stderr}");
            for (index, line) in stages.iter().enumerate() {
                assert!(line.starts_with(&format!("[{}/4]", index + 1)));
            }
            assert_eq!(stderr.matches("Generation complete ·").count(), 1);
            assert!(stderr.contains("Cumulative tokens 12,800 (input 12,000 / output 800)"));
            let cache = if agent == "agy" {
                "Input cache breakdown unavailable"
            } else {
                "Input cache 10,000 / non-cached 2,000"
            };
            assert!(
                stderr.contains(&format!("{cache} · Tool calls 2")),
                "{agent}: {stderr}"
            );
            assert!(stderr.find("[3/4]").unwrap() < stderr.find("Generation complete").unwrap());
            assert!(stderr.find("Generation complete").unwrap() < stderr.find("[4/4]").unwrap());
            assert!(stderr.find("Input cache").unwrap() < stderr.find("[4/4]").unwrap());
            assert!(!stdout.contains("Generation complete"));
            assert!(stdout.contains("Show the approved result"));
            assert!(stdout.contains("prints the approved result"));
            assert!(stdout.contains("risk"));
            assert!(stdout.contains("network"));
            if persistent {
                assert!(stderr.contains("installation approval"));
                assert!(stdout.contains("Installed:"));
                assert!(!stdout.contains("PUBLIC_RESULT"));
                assert!(fixture.root.join("source/commands/quiet-command").is_dir());
                assert!(fixture.root.join("data/current").is_file());
                assert_eq!(
                    fs::read_dir(fixture.root.join("data/history"))
                        .unwrap()
                        .count(),
                    0
                );
            } else {
                assert!(stderr.contains("execution approval"));
                assert!(stdout.contains("PUBLIC_RESULT"));
                assert!(stdout.contains(fixture.root.join("caller").to_str().unwrap()));
                assert!(stdout.contains("/package/quiet-command"));
                assert!(!fixture.root.join("source").exists());
                assert!(!fixture.root.join("data/current").exists());
            }
            fixture.assert_workspace_cleaned();
        }
    }
}

#[test]
fn provider_failure_is_concise_and_never_replays_the_transcript_or_reaches_approval() {
    for agent in ["codex", "claude", "agy"] {
        for persistent in [false, true] {
            let fixture = Fixture::new();
            let output = fixture
                .command(persistent, agent)
                .args(["--yes", "--no-input"])
                .env("SCV_TEST_FAIL", "1")
                .output()
                .unwrap();
            let (stdout, stderr) = visible_output(&output);
            assert!(!output.status.success());
            assert!(stdout.is_empty());
            assert!(stderr.contains(&format!("agent '{agent}' failed")));
            assert!(stderr.contains("17"), "{agent}: {stderr}");
            assert!(stderr.contains("— failed"));
            assert!(!stderr.contains("[3/4]"));
            assert!(!stderr.contains("[4/4]"));
            assert!(!fixture.root.join("source").exists());
            assert!(!fixture.root.join("data").exists());
            fixture.assert_workspace_cleaned();
        }
    }
}

#[test]
fn invalid_generated_package_fails_at_validation_before_approval_or_mutation() {
    for persistent in [false, true] {
        let fixture = Fixture::new();
        fs::write(fixture.root.join("fixture/metadata.toml"), "name = 3\n").unwrap();
        let output = fixture
            .command(persistent, "codex")
            .args(["--yes", "--no-input"])
            .output()
            .unwrap();
        let (_, stderr) = visible_output(&output);
        assert!(!output.status.success());
        assert!(stderr.contains("[3/4] Validating command — failed"));
        assert!(!stderr.contains("[4/4]"));
        assert!(!fixture.root.join("source").exists());
        assert!(!fixture.root.join("data").exists());
        fixture.assert_workspace_cleaned();
    }
}

#[test]
fn disabled_input_does_not_start_an_agent_without_separate_consent() {
    for persistent in [false, true] {
        for no_input in [false, true] {
            let fixture = Fixture::new();
            let mut command = fixture.command(persistent, "codex");
            if no_input {
                command.arg("--no-input");
            }
            let output = command.output().unwrap();
            let (_, stderr) = visible_output(&output);
            assert!(!output.status.success());
            assert!(stderr.contains("--yes is required"));
            assert!(!stderr.contains("[1/4]"));
            assert!(!fixture.root.join("workspace").exists());
            assert!(!fixture.root.join("source").exists());
            assert!(!fixture.root.join("data").exists());
        }
    }
}

#[test]
fn progress_uses_the_ui_locale_independently_of_the_generated_package_locale() {
    for persistent in [false, true] {
        let fixture = Fixture::new();
        fs::write(
            fixture.root.join("config/config.toml"),
            "[ui]\nlocale = 'ko'\n",
        )
        .unwrap();
        let output = fixture
            .command(persistent, "codex")
            .args(["--yes", "--no-input", "--locale", "en"])
            .output()
            .unwrap();
        let (_, stderr) = visible_output(&output);
        assert!(output.status.success(), "{stderr}");
        assert!(stderr.contains("[1/4] 요청 준비 중"));
        assert!(stderr.contains("[2/4] codex로 명령 생성 중"));
        assert!(stderr.contains("승인 준비 완료"));
        assert!(stderr.contains("생성 완료 ·"));
        assert!(stderr.contains("누적 토큰 12,800 (입력 12,000 / 출력 800)"));
        assert!(stderr.contains("입력 캐시 10,000 / 비캐시 2,000 · 도구 호출 2회"));
        let prompt = fs::read_to_string(fixture.root.join("prompt")).unwrap();
        assert!(prompt.contains("human-facing text in English (en)"));
    }
}

#[test]
fn missing_usage_is_explicit_and_does_not_block_generation() {
    for agent in ["codex", "claude", "agy"] {
        for persistent in [false, true] {
            let fixture = Fixture::new();
            let output = fixture
                .command(persistent, agent)
                .args(["--yes", "--no-input"])
                .env("SCV_TEST_MISSING_USAGE", "1")
                .output()
                .unwrap();
            let (_, stderr) = visible_output(&output);
            assert!(output.status.success(), "{stderr}");
            assert!(stderr.contains("Generation complete ·"));
            assert!(stderr.contains("Tokens unavailable"));
            assert!(stderr.contains("Input cache breakdown unavailable"));
            assert!(stderr.contains("Tool call count unavailable"));
            fixture.assert_workspace_cleaned();
        }
    }
}

#[test]
fn malformed_stdout_preserves_completion_totals_without_guessing_tool_calls() {
    for agent in ["codex", "claude", "agy"] {
        for persistent in [false, true] {
            let fixture = Fixture::new();
            let output = fixture
                .command(persistent, agent)
                .args(["--yes", "--no-input"])
                .env("SCV_TEST_MALFORMED_STREAM", "1")
                .output()
                .unwrap();
            let (_, stderr) = visible_output(&output);
            assert!(output.status.success(), "{agent}: {stderr}");
            assert!(stderr.contains("Cumulative tokens 12,800 (input 12,000 / output 800)"));
            assert!(
                stderr.contains("Tool call count unavailable"),
                "{agent}: {stderr}"
            );
            assert!(!stderr.contains("Tool calls 2"));
            assert!(stderr.contains("[4/4]"));
            fixture.assert_workspace_cleaned();
        }
    }
}

#[test]
fn structured_failure_with_success_exit_never_reaches_approval() {
    for agent in ["codex", "claude", "agy"] {
        for persistent in [false, true] {
            let fixture = Fixture::new();
            let output = fixture
                .command(persistent, agent)
                .args(["--yes", "--no-input"])
                .env("SCV_TEST_RESULT_ERROR", "1")
                .output()
                .unwrap();
            let (stdout, stderr) = visible_output(&output);
            assert!(!output.status.success());
            assert!(stdout.is_empty());
            assert!(stderr.contains("reported a failed generation"));
            assert!(!stderr.contains("[3/4]"));
            assert!(!stderr.contains("Generation complete"));
            assert!(!fixture.root.join("source").exists());
            assert!(!fixture.root.join("data").exists());
            fixture.assert_workspace_cleaned();
        }
    }
}

#[test]
fn embedded_native_example_handles_empty_and_unusual_directory_names_after_approval() {
    for has_directories in [false, true] {
        let fixture = Fixture::new();
        fixture.use_directory_size_example();
        let caller = fixture.root.join("caller");
        fs::write(caller.join("ordinary-file"), b"not a directory").unwrap();
        fs::create_dir(caller.join(".hidden-directory")).unwrap();
        if has_directories {
            for (name, size) in [("-small folder", 8192), ("large folder", 131072)] {
                fs::create_dir(caller.join(name)).unwrap();
                fs::write(caller.join(name).join("payload"), vec![b'x'; size]).unwrap();
            }
        }
        let output = fixture
            .command(false, "codex")
            .args(["--yes", "--no-input"])
            .output()
            .unwrap();
        let (stdout, stderr) = visible_output(&output);
        assert!(output.status.success(), "{stderr}");
        let (_, result) = stdout
            .split_once("Running in the current working directory...\n")
            .unwrap();
        if has_directories {
            let paths = result
                .lines()
                .map(|line| line.split_once('\t').unwrap().1)
                .collect::<Vec<_>>();
            assert_eq!(paths, ["./-small folder/", "./large folder/"]);
        } else {
            assert!(result.is_empty(), "{result}");
        }
        assert!(!fixture.root.join("source").exists());
        let stored = fs::read_dir(fixture.root.join("data/history"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path()
            .join("package/quiet-command/main.sh");
        assert_eq!(
            fs::read_to_string(stored).unwrap(),
            fs::read_to_string(fixture.root.join("fixture/main.sh")).unwrap()
        );
        fixture.assert_workspace_cleaned();
    }
}

#[test]
fn embedded_native_example_preserves_utility_errors_and_pipeline_failure() {
    let fixture = Fixture::new();
    fixture.use_directory_size_example();
    fs::create_dir(fixture.root.join("caller/folder")).unwrap();
    let utility = fixture.root.join("agents/du");
    fs::write(
        &utility,
        "#!/bin/sh\nprintf 'UTILITY_FAILURE\\n%s\\n' \"$SCV_COMMAND_PACKAGE_DIR\" >&2\nexit 17\n",
    )
    .unwrap();
    fs::set_permissions(utility, fs::Permissions::from_mode(0o700)).unwrap();
    let output = fixture
        .command(false, "codex")
        .args(["--yes", "--no-input", "--locale", "ko"])
        .output()
        .unwrap();
    let (_, stderr) = visible_output(&output);
    assert_eq!(output.status.code(), Some(17), "{stderr}");
    assert_eq!(stderr.matches("UTILITY_FAILURE").count(), 1);
    assert!(stderr.contains(fixture.root.join("data/history").to_str().unwrap()));
    assert!(stderr.contains("/package/quiet-command"));
    fixture.assert_workspace_cleaned();
}
