use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::activation::{self, ApplyResult};
use crate::paths::AppPaths;
use crate::source;
use crate::storage;

#[derive(Debug, Serialize)]
pub struct GitStatus {
    pub repository: String,
    pub branch: String,
    pub head: String,
    pub upstream: Option<String>,
    pub dirty: bool,
    pub ahead: u64,
    pub behind: u64,
}

#[derive(Debug, Serialize)]
pub struct PullResult {
    pub changed: bool,
    pub from: String,
    pub to: String,
    pub activation: Option<ApplyResult>,
}

pub fn status(paths: &AppPaths) -> Result<GitStatus, String> {
    ensure_repository(&paths.source_home)?;
    let branch = git_text(&paths.source_home, &["symbolic-ref", "--short", "HEAD"])?;
    let head = git_text(&paths.source_home, &["rev-parse", "HEAD"])?;
    let dirty = !git_text(&paths.source_home, &["status", "--porcelain"])?.is_empty();
    let upstream = git_optional_text(
        &paths.source_home,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    );
    let (ahead, behind) = if upstream.is_some() {
        let counts = git_text(
            &paths.source_home,
            &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
        )?;
        parse_counts(&counts)?
    } else {
        (0, 0)
    };
    Ok(GitStatus {
        repository: paths.source_home.to_string_lossy().into_owned(),
        branch,
        head,
        upstream,
        dirty,
        ahead,
        behind,
    })
}

pub fn prepare_pull(paths: &AppPaths) -> Result<PullPreview, String> {
    source::validate(paths)?;
    ensure_clean(paths)?;
    let branch = git_text(&paths.source_home, &["symbolic-ref", "--short", "HEAD"])?;
    git(&paths.source_home, &["fetch", "--prune", "origin"])?;
    let remote_ref = format!("refs/remotes/origin/{branch}");
    let from = git_text(&paths.source_home, &["rev-parse", "HEAD"])?;
    let to = git_text(&paths.source_home, &["rev-parse", &remote_ref])?;
    if from == to {
        return Ok(PullPreview {
            from,
            to,
            worktree: None,
            summary: "Already up to date.".to_string(),
        });
    }
    if !git_success(
        &paths.source_home,
        &["merge-base", "--is-ancestor", "HEAD", &remote_ref],
    ) {
        return Err(
            "sync pull requires a fast-forward; reconcile local and remote history with Git"
                .to_string(),
        );
    }

    fs::create_dir_all(&paths.cache_dir).map_err(|error| {
        format!(
            "could not create SCV cache directory '{}': {error}",
            paths.cache_dir.display()
        )
    })?;
    let source_root = fs::canonicalize(&paths.source_home).map_err(|error| {
        format!(
            "could not resolve SCV source '{}': {error}",
            paths.source_home.display()
        )
    })?;
    let cache_root = fs::canonicalize(&paths.cache_dir).map_err(|error| {
        format!(
            "could not resolve SCV cache '{}': {error}",
            paths.cache_dir.display()
        )
    })?;
    if cache_root.starts_with(&source_root) {
        return Err("SCV_CACHE_DIR must not be inside the Git source repository".to_string());
    }
    let worktree = paths.cache_dir.join(format!(
        "sync-worktree-{}-{}",
        std::process::id(),
        crate::package::nonce()
    ));
    let output = Command::new("git")
        .arg("-C")
        .arg(&paths.source_home)
        .args(["worktree", "add", "--detach"])
        .arg(&worktree)
        .arg(&remote_ref)
        .output()
        .map_err(|error| format!("could not start Git: {error}"))?;
    if !output.status.success() {
        return Err(git_error(&["worktree", "add", "--detach"], &output));
    }
    let validation = (|| {
        source::validate_home(&worktree)?;
        activation::inspect_library(&worktree.join("commands"))?;
        git_text(&paths.source_home, &["diff", "--stat", "HEAD", &remote_ref])
    })();
    match validation {
        Ok(summary) => Ok(PullPreview {
            from,
            to,
            worktree: Some(worktree),
            summary,
        }),
        Err(error) => {
            cleanup_worktree(&paths.source_home, &worktree);
            Err(error)
        }
    }
}

pub fn commit_pull(paths: &AppPaths, preview: PullPreview) -> Result<PullResult, String> {
    if preview.from == preview.to {
        return Ok(PullResult {
            changed: false,
            from: preview.from,
            to: preview.to,
            activation: None,
        });
    }
    ensure_clean(paths)?;
    let current = git_text(&paths.source_home, &["rev-parse", "HEAD"])?;
    if current != preview.from {
        cleanup_preview(paths, &preview);
        return Err("source repository changed while sync pull was awaiting approval".to_string());
    }
    let merge = git(&paths.source_home, &["merge", "--ff-only", &preview.to]);
    cleanup_preview(paths, &preview);
    merge?;
    let activation = activation::apply(paths).map_err(|error| {
        format!(
            "source was fast-forwarded but activation failed; the previous activation remains active: {error}"
        )
    })?;
    Ok(PullResult {
        changed: true,
        from: preview.from,
        to: preview.to,
        activation: Some(activation),
    })
}

pub fn push(paths: &AppPaths, dry_run: bool) -> Result<(), String> {
    source::validate(paths)?;
    ensure_clean(paths)?;
    ensure_repository(&paths.source_home)?;
    if dry_run {
        git(&paths.source_home, &["push", "--dry-run", "origin", "HEAD"])
    } else {
        git(&paths.source_home, &["push", "origin", "HEAD"])
    }
}

pub struct PullPreview {
    pub from: String,
    pub to: String,
    worktree: Option<PathBuf>,
    pub summary: String,
}

pub fn cleanup_preview(paths: &AppPaths, preview: &PullPreview) {
    if let Some(worktree) = &preview.worktree {
        cleanup_worktree(&paths.source_home, worktree);
    }
}

fn ensure_repository(repository: &Path) -> Result<(), String> {
    if !repository.join(".git").exists() {
        return Err(format!(
            "SCV source is not a Git repository: {}",
            repository.display()
        ));
    }
    git_text(repository, &["rev-parse", "--is-inside-work-tree"]).and_then(|value| {
        (value == "true")
            .then_some(())
            .ok_or_else(|| "invalid Git worktree".to_string())
    })
}

fn ensure_clean(paths: &AppPaths) -> Result<(), String> {
    ensure_repository(&paths.source_home)?;
    if !git_text(&paths.source_home, &["status", "--porcelain"])?.is_empty() {
        return Err(
            "SCV source has uncommitted changes; commit or discard them before syncing".to_string(),
        );
    }
    Ok(())
}

fn parse_counts(value: &str) -> Result<(u64, u64), String> {
    let mut fields = value.split_whitespace();
    let ahead = fields
        .next()
        .ok_or_else(|| "Git did not report ahead count".to_string())?
        .parse::<u64>()
        .map_err(|error| format!("invalid Git ahead count: {error}"))?;
    let behind = fields
        .next()
        .ok_or_else(|| "Git did not report behind count".to_string())?
        .parse::<u64>()
        .map_err(|error| format!("invalid Git behind count: {error}"))?;
    Ok((ahead, behind))
}

fn git(repository: &Path, arguments: &[&str]) -> Result<(), String> {
    let output = git_output(repository, arguments)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_error(arguments, &output))
    }
}

fn git_text(repository: &Path, arguments: &[&str]) -> Result<String, String> {
    let output = git_output(repository, arguments)?;
    if !output.status.success() {
        return Err(git_error(arguments, &output));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_optional_text(repository: &Path, arguments: &[&str]) -> Option<String> {
    git_text(repository, arguments)
        .ok()
        .filter(|value| !value.is_empty())
}

fn git_success(repository: &Path, arguments: &[&str]) -> bool {
    git_output(repository, arguments).is_ok_and(|output| output.status.success())
}

fn git_output(repository: &Path, arguments: &[&str]) -> Result<Output, String> {
    Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                "Git executable was not found".to_string()
            } else {
                format!("could not start Git: {error}")
            }
        })
}

fn git_error(arguments: &[&str], output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.trim().chars().take(1200).collect::<String>();
    format!("git {} failed: {detail}", arguments.join(" "))
}

fn cleanup_worktree(repository: &Path, worktree: &Path) {
    let _ = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(["worktree", "remove", "--force"])
        .arg(worktree)
        .output();
    if worktree.exists() {
        let _ = storage::remove_if_exists(worktree);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    use crate::paths::AppPaths;

    fn run(directory: &Path, arguments: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(directory)
            .args(arguments)
            .output()
            .expect("Git should start");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn write_library(root: &Path, body: &str) {
        fs::create_dir_all(root.join("commands/sample")).expect("commands should be created");
        fs::write(
            root.join("scv.toml"),
            "schema_version = 1\n\n[library]\nformat = \"command-packages\"\n",
        )
        .expect("source manifest should be written");
        fs::write(
            root.join("commands/sample/metadata.toml"),
            r#"name = "sample"
category = "test"
description = "sample command"
usage = "scv sample"
builtin = false
risk = "read"
network = false
supports_dry_run = false
effects = ["reads sample input"]

[[implementations]]
runtime = "python"
platforms = ["windows", "linux", "macos"]
entry = "main.py"
"#,
        )
        .expect("metadata should be written");
        fs::write(root.join("commands/sample/main.py"), body).expect("entry should be written");
    }

    #[test]
    fn pulls_through_an_isolated_validated_worktree() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root =
            std::env::temp_dir().join(format!("scv-sync-test-{}", crate::storage::unique_nonce()));
        let publisher = root.join("publisher");
        let remote = root.join("remote.git");
        fs::create_dir_all(&publisher).expect("publisher should be created");
        run(&publisher, &["init", "-b", "main"]);
        run(&publisher, &["config", "user.name", "SCV Test"]);
        run(&publisher, &["config", "user.email", "scv@example.invalid"]);
        write_library(&publisher, "print('one')\n");
        run(&publisher, &["add", "."]);
        run(&publisher, &["commit", "-m", "initial"]);
        fs::create_dir_all(&remote).expect("remote should be created");
        run(&remote, &["init", "--bare"]);
        run(
            &publisher,
            &[
                "remote",
                "add",
                "origin",
                remote.to_str().expect("UTF-8 path"),
            ],
        );
        run(&publisher, &["push", "-u", "origin", "main"]);

        let paths = AppPaths::isolated(root.join("client")).expect("paths should resolve");
        fs::create_dir_all(paths.source_home.parent().expect("source parent"))
            .expect("source parent should be created");
        let output = Command::new("git")
            .args(["clone", "--branch", "main"])
            .arg(&remote)
            .arg(&paths.source_home)
            .output()
            .expect("Git clone should start");
        assert!(output.status.success());
        let first = crate::activation::apply(&paths).expect("initial source should apply");

        fs::write(publisher.join("commands/sample/main.py"), "print('two')\n")
            .expect("publisher source should change");
        run(&publisher, &["add", "."]);
        run(&publisher, &["commit", "-m", "update"]);
        run(&publisher, &["push", "origin", "main"]);

        let preview = super::prepare_pull(&paths).expect("pull should validate");
        assert_ne!(preview.from, preview.to);
        let result = super::commit_pull(&paths, preview).expect("pull should commit");
        assert!(result.changed);
        let second = result.activation.expect("pull should activate");
        assert_ne!(first.activation, second.activation);
        assert_eq!(
            fs::read_to_string(paths.source_package_dir("sample").join("main.py"))
                .expect("source entry should read"),
            "print('two')\n"
        );
        fs::remove_dir_all(root).expect("fixture should be removed");
    }
}
