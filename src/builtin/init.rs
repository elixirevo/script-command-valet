use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Output};

use crate::agent;
use crate::config::Settings;
use crate::i18n::{I18n, Locale};
use crate::input;
use crate::paths::AppPaths;
use crate::source;

#[derive(Default)]
struct InitOptions {
    locale: Option<String>,
    agent: Option<String>,
    remote: Option<String>,
    dry_run: bool,
    no_input: bool,
}

struct InitPlan {
    locale: Locale,
    agent: String,
    remote: Option<String>,
    initialize_git: bool,
    add_origin: bool,
}

pub fn run(arguments: &[OsString], paths: &AppPaths, current_i18n: &I18n) -> Result<i32, String> {
    if paths.source_manifest_path().exists() {
        return Err(current_i18n.format(
            "init.already_initialized",
            &[("path", &paths.source_manifest_path().display().to_string())],
        ));
    }

    let options = parse(arguments)?;
    let mut settings = Settings::load(paths)?;
    let interactive = input::is_enabled(options.no_input);
    let locale = match options.locale.as_deref() {
        Some(value) => Locale::parse(value)?,
        None if interactive => prompt_locale(settings.ui.locale)?,
        None => settings.ui.locale,
    };
    let i18n = I18n::new(locale)?;
    let default_agent = settings
        .create
        .agent
        .as_deref()
        .unwrap_or("codex")
        .to_string();
    let selected_agent = match options.agent {
        Some(agent) => agent,
        None if interactive => prompt_agent(&i18n, &default_agent)?,
        None => default_agent,
    };
    agent::adapter(&selected_agent).map_err(|error| format!("init: {error}"))?;
    let remote = match options.remote {
        Some(remote) => Some(remote),
        None if interactive => prompt_remote(&i18n)?,
        None => None,
    };
    if let Some(remote) = remote.as_deref() {
        validate_remote(remote)?;
    }
    let repository = inspect_repository(paths, remote.as_deref())?;
    let plan = InitPlan {
        locale,
        agent: selected_agent,
        remote,
        initialize_git: repository.initialize,
        add_origin: repository.add_origin,
    };

    print_plan(&plan, paths, &i18n, options.dry_run);
    if options.dry_run {
        println!("{}", i18n.text("common.no_files_changed"));
        return Ok(0);
    }

    source::ensure_initialized(paths).map_err(|error| format!("init: {error}"))?;
    apply_repository(paths, &plan)?;
    settings.ui.locale = locale;
    settings.create.agent = Some(plan.agent.clone());
    settings
        .save(paths)
        .map_err(|error| format!("init: {error}"))?;

    println!();
    println!("{}", i18n.text("init.complete"));
    println!(
        "  {} : {}",
        i18n.text("init.source"),
        paths.source_home.display()
    );
    println!(
        "  {} : {}",
        i18n.text("init.config"),
        paths.config_path().display()
    );
    println!("{}", i18n.text("init.next_steps"));
    Ok(0)
}

fn parse(arguments: &[OsString]) -> Result<InitOptions, String> {
    let mut options = InitOptions::default();
    let mut index = 0;
    while index < arguments.len() {
        let argument = arguments[index]
            .to_str()
            .ok_or_else(|| "init: arguments must be valid UTF-8".to_string())?;
        match argument {
            "--locale" => set_option(
                &mut options.locale,
                argument,
                value(arguments, &mut index, argument)?,
            )?,
            "--agent" => set_option(
                &mut options.agent,
                argument,
                value(arguments, &mut index, argument)?,
            )?,
            "--remote" => set_option(
                &mut options.remote,
                argument,
                value(arguments, &mut index, argument)?,
            )?,
            "--dry-run" => set_flag(&mut options.dry_run, argument)?,
            "--no-input" => set_flag(&mut options.no_input, argument)?,
            value => return Err(usage_error(&format!("unknown argument '{value}'"))),
        }
        index += 1;
    }
    Ok(options)
}

fn prompt_locale(default: Locale) -> Result<Locale, String> {
    loop {
        let answer = prompt(&format!(
            "SCV language / 언어 [en/ko] ({}): ",
            default.as_str()
        ))?;
        if answer.is_empty() {
            return Ok(default);
        }
        match answer.as_str() {
            "1" | "en" | "English" | "english" => return Ok(Locale::En),
            "2" | "ko" | "한국어" => return Ok(Locale::Ko),
            _ => eprintln!("Choose en or ko. / en 또는 ko를 선택하세요."),
        }
    }
}

fn prompt_agent(i18n: &I18n, default: &str) -> Result<String, String> {
    loop {
        let answer = prompt(&i18n.format("init.agent_prompt", &[("default", default)]))?;
        let selected = if answer.is_empty() {
            default.to_string()
        } else {
            answer
        };
        if agent::adapter(&selected).is_ok() {
            return Ok(selected);
        }
        eprintln!("{}", i18n.text("init.invalid_agent"));
    }
}

fn prompt_remote(i18n: &I18n) -> Result<Option<String>, String> {
    let answer = prompt(i18n.text("init.remote_prompt"))?;
    Ok((!answer.is_empty()).then_some(answer))
}

fn prompt(message: &str) -> Result<String, String> {
    print!("{message}");
    io::stdout()
        .flush()
        .map_err(|error| format!("init: could not write prompt: {error}"))?;
    let mut answer = String::new();
    let bytes = io::stdin()
        .read_line(&mut answer)
        .map_err(|error| format!("init: could not read input: {error}"))?;
    if bytes == 0 {
        return Err("init: input ended before setup completed".to_string());
    }
    Ok(answer.trim().to_string())
}

struct RepositoryPlan {
    initialize: bool,
    add_origin: bool,
}

fn inspect_repository(
    paths: &AppPaths,
    requested_remote: Option<&str>,
) -> Result<RepositoryPlan, String> {
    git_global(&["--version"])?;
    let git_path = paths.source_home.join(".git");
    let initialized = match fs::symlink_metadata(&git_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "init: Git metadata must not be a symbolic link: {}",
                git_path.display()
            ));
        }
        Ok(_) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(format!(
                "init: could not inspect Git metadata '{}': {error}",
                git_path.display()
            ));
        }
    };
    if !initialized {
        return Ok(RepositoryPlan {
            initialize: true,
            add_origin: requested_remote.is_some(),
        });
    }
    let inside = git_text(&paths.source_home, &["rev-parse", "--is-inside-work-tree"])?;
    if inside != "true" {
        return Err("init: SCV_HOME contains invalid Git metadata".to_string());
    }
    let remotes = git_text(&paths.source_home, &["remote"])?;
    let existing = if remotes.lines().any(|remote| remote == "origin") {
        Some(git_text(
            &paths.source_home,
            &["remote", "get-url", "origin"],
        )?)
    } else {
        None
    };
    match (requested_remote, existing.as_deref()) {
        (Some(requested), Some(existing)) if requested != existing => Err(format!(
            "init: Git remote 'origin' is already set to '{existing}'"
        )),
        (Some(_), None) => Ok(RepositoryPlan {
            initialize: false,
            add_origin: true,
        }),
        _ => Ok(RepositoryPlan {
            initialize: false,
            add_origin: false,
        }),
    }
}

fn apply_repository(paths: &AppPaths, plan: &InitPlan) -> Result<(), String> {
    if plan.initialize_git {
        git(&paths.source_home, &["init"])?;
    }
    if plan.add_origin {
        let remote = plan
            .remote
            .as_deref()
            .ok_or_else(|| "init: missing remote for Git origin".to_string())?;
        git(&paths.source_home, &["remote", "add", "origin", remote])?;
    }
    Ok(())
}

fn validate_remote(remote: &str) -> Result<(), String> {
    if remote.is_empty()
        || remote.len() > 4096
        || remote.starts_with('-')
        || remote.chars().any(char::is_control)
    {
        return Err("init: remote must be a non-empty, single-line Git URL or path".to_string());
    }
    if let Some((scheme, remainder)) = remote.split_once("://")
        && matches!(scheme, "http" | "https")
    {
        let authority = remainder.split(['/', '?', '#']).next().unwrap_or("");
        if authority.contains('@') || remainder.contains(['?', '#']) {
            return Err(
                "init: HTTP(S) remote must not contain embedded credentials, a query, or a fragment"
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn print_plan(plan: &InitPlan, paths: &AppPaths, i18n: &I18n, dry_run: bool) {
    println!(
        "{}",
        i18n.text(if dry_run {
            "init.plan"
        } else {
            "init.setting_up"
        })
    );
    println!("  {} : {}", i18n.text("init.locale"), plan.locale.as_str());
    println!("  {} : {}", i18n.text("init.agent"), plan.agent);
    println!(
        "  {} : {}",
        i18n.text("init.source"),
        paths.source_home.display()
    );
    println!(
        "  {} : {}",
        i18n.text("init.config"),
        paths.config_path().display()
    );
    println!(
        "  {} : {}",
        i18n.text("init.remote"),
        plan.remote
            .as_deref()
            .unwrap_or_else(|| i18n.text("common.none"))
    );
}

fn set_option<T>(target: &mut Option<T>, option: &str, value: T) -> Result<(), String> {
    if target.is_some() {
        return Err(usage_error(&format!("duplicate option '{option}'")));
    }
    *target = Some(value);
    Ok(())
}

fn set_flag(target: &mut bool, option: &str) -> Result<(), String> {
    if *target {
        return Err(usage_error(&format!("duplicate option '{option}'")));
    }
    *target = true;
    Ok(())
}

fn value(arguments: &[OsString], index: &mut usize, option: &str) -> Result<String, String> {
    *index += 1;
    arguments
        .get(*index)
        .and_then(|value| value.to_str())
        .map(str::to_string)
        .ok_or_else(|| usage_error(&format!("{option} requires a value")))
}

fn usage_error(message: &str) -> String {
    format!("init: {message}. Try 'scv init --help' for more information")
}

fn git(repository: &Path, arguments: &[&str]) -> Result<(), String> {
    let output = git_output(repository, arguments)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_error(arguments, &output))
    }
}

fn git_global(arguments: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(arguments)
        .output()
        .map_err(git_start_error)?;
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

fn git_output(repository: &Path, arguments: &[&str]) -> Result<Output, String> {
    Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .map_err(git_start_error)
}

fn git_start_error(error: io::Error) -> String {
    if error.kind() == io::ErrorKind::NotFound {
        "init: Git executable was not found".to_string()
    } else {
        format!("init: could not start Git: {error}")
    }
}

fn git_error(arguments: &[&str], output: &Output) -> String {
    let detail = String::from_utf8_lossy(&output.stderr)
        .trim()
        .chars()
        .take(1200)
        .collect::<String>();
    format!("init: git {} failed: {detail}", arguments.join(" "))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;
    use std::process::Command;

    use super::{parse, run, validate_remote};
    use crate::config::Settings;
    use crate::i18n::{I18n, Locale};
    use crate::paths::AppPaths;

    #[test]
    fn parses_fully_specified_noninteractive_setup() {
        let arguments = [
            "--locale",
            "ko",
            "--agent",
            "agy",
            "--remote",
            "git@github.com:owner/commands.git",
            "--no-input",
        ]
        .map(OsString::from);
        let options = parse(&arguments).expect("arguments should parse");
        assert_eq!(options.locale.as_deref(), Some("ko"));
        assert_eq!(options.agent.as_deref(), Some("agy"));
        assert!(options.no_input);
    }

    #[test]
    fn rejects_credentials_in_http_remotes() {
        assert!(validate_remote("https://github.com/owner/commands.git").is_ok());
        assert!(validate_remote("git@github.com:owner/commands.git").is_ok());
        assert!(validate_remote("https://token@github.com/owner/commands.git").is_err());
        assert!(validate_remote("https://github.com/owner/commands.git?token=value").is_err());
    }

    #[test]
    fn initializes_source_config_and_git_origin() {
        let root =
            std::env::temp_dir().join(format!("scv-init-test-{}", crate::storage::unique_nonce()));
        let paths = AppPaths::isolated(root.clone()).expect("paths should resolve");
        let remote = root.join("upload.git").to_string_lossy().into_owned();
        let arguments = [
            "--locale",
            "ko",
            "--agent",
            "agy",
            "--remote",
            &remote,
            "--no-input",
        ]
        .map(OsString::from);
        let i18n = I18n::new(Locale::En).expect("catalog should load");

        run(&arguments, &paths, &i18n).expect("init should succeed");

        crate::source::validate(&paths).expect("source should validate");
        let settings = Settings::load(&paths).expect("settings should load");
        assert_eq!(settings.ui.locale, Locale::Ko);
        assert_eq!(settings.create.agent.as_deref(), Some("agy"));
        let output = Command::new("git")
            .arg("-C")
            .arg(&paths.source_home)
            .args(["remote", "get-url", "origin"])
            .output()
            .expect("Git should start");
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), remote);
        fs::remove_dir_all(root).expect("fixture should be removed");
    }

    #[test]
    fn dry_run_does_not_create_source_or_config() {
        let root = std::env::temp_dir().join(format!(
            "scv-init-dry-run-test-{}",
            crate::storage::unique_nonce()
        ));
        let paths = AppPaths::isolated(root.clone()).expect("paths should resolve");
        let arguments = [
            "--locale",
            "en",
            "--agent",
            "codex",
            "--dry-run",
            "--no-input",
        ]
        .map(OsString::from);
        let i18n = I18n::new(Locale::En).expect("catalog should load");

        run(&arguments, &paths, &i18n).expect("dry-run should succeed");

        assert!(!paths.source_home.exists());
        assert!(!paths.config_path().exists());
        if root.exists() {
            fs::remove_dir_all(root).expect("fixture should be removed");
        }
    }

    #[test]
    fn preserves_an_existing_conflicting_origin() {
        let root = std::env::temp_dir().join(format!(
            "scv-init-origin-test-{}",
            crate::storage::unique_nonce()
        ));
        let paths = AppPaths::isolated(root.clone()).expect("paths should resolve");
        fs::create_dir_all(&paths.source_home).expect("source directory should exist");
        for arguments in [
            &["init"][..],
            &["remote", "add", "origin", "existing.git"][..],
        ] {
            let output = Command::new("git")
                .arg("-C")
                .arg(&paths.source_home)
                .args(arguments)
                .output()
                .expect("Git should start");
            assert!(output.status.success());
        }
        let arguments = [
            "--locale",
            "en",
            "--agent",
            "codex",
            "--remote",
            "replacement.git",
            "--no-input",
        ]
        .map(OsString::from);
        let i18n = I18n::new(Locale::En).expect("catalog should load");

        let error = run(&arguments, &paths, &i18n).expect_err("conflict should be rejected");

        assert!(error.contains("already set to 'existing.git'"));
        assert!(!paths.source_manifest_path().exists());
        assert!(!paths.config_path().exists());
        let output = Command::new("git")
            .arg("-C")
            .arg(&paths.source_home)
            .args(["remote", "get-url", "origin"])
            .output()
            .expect("Git should start");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "existing.git"
        );
        fs::remove_dir_all(root).expect("fixture should be removed");
    }
}
