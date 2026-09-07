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
    printf '\033[31mHIDDEN_TRANSCRIPT source and tool output\033[0m\n'
    printf 'HIDDEN_TRANSCRIPT reasoning and diagnostics\n' >&2
    i=$((i + 1))
done
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
            let stages = stderr.lines().collect::<Vec<_>>();
            assert_eq!(stages.len(), 4, "{stderr}");
            for (index, line) in stages.iter().enumerate() {
                assert!(line.starts_with(&format!("[{}/4]", index + 1)));
            }
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
        let prompt = fs::read_to_string(fixture.root.join("prompt")).unwrap();
        assert!(prompt.contains("human-facing text in English (en)"));
    }
}
