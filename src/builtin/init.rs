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

#[derive(Debug, Default)]
struct InitOptions {
    locale: Option<String>,
    agent: Option<String>,
    remote: Option<String>,
    reconfigure: bool,
    no_remote: bool,
    dry_run: bool,
    no_input: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RemoteSelection {
    Keep,
    Set(String),
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteAction {
    None,
    Add,
    SetUrl,
    Remove,
}

struct RepositoryState {
    initialize: bool,
    origin: Option<String>,
}

struct InitPlan {
    locale: Locale,
    agent: String,
    remote: Option<String>,
    initialize_git: bool,
    remote_action: RemoteAction,
    reconfigure: bool,
}

pub fn run(arguments: &[OsString], paths: &AppPaths, current_i18n: &I18n) -> Result<i32, String> {
    let options = parse(arguments)?;
    let initialized = paths.source_manifest_path().exists();
    if initialized && !options.reconfigure {
        return Err(current_i18n.format(
            "init.already_initialized",
            &[("path", &paths.source_manifest_path().display().to_string())],
        ));
    }
    if !initialized && options.reconfigure {
        return Err(current_i18n.text("init.not_initialized").to_string());
    }
    if options.no_remote && !options.reconfigure {
        return Err(usage_error("--no-remote requires --reconfigure"));
    }

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
    let repository = inspect_repository(paths)?;
    let remote_selection = match options.remote {
        Some(remote) => RemoteSelection::Set(remote),
        None if options.no_remote => RemoteSelection::Remove,
        None if interactive => {
            prompt_remote(&i18n, repository.origin.as_deref(), options.reconfigure)?
        }
        None => RemoteSelection::Keep,
    };
    if let RemoteSelection::Set(remote) = &remote_selection {
        validate_remote(remote)?;
    }
    let (remote, remote_action) =
        resolve_remote(repository.origin, remote_selection, options.reconfigure)?;
    let plan = InitPlan {
        locale,
        agent: selected_agent,
        remote,
        initialize_git: repository.initialize,
        remote_action,
        reconfigure: options.reconfigure,
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
    println!(
        "{}",
        i18n.text(if plan.reconfigure {
            "init.reconfigure_complete"
        } else {
            "init.complete"
        })
    );
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
            "--reconfigure" => set_flag(&mut options.reconfigure, argument)?,
            "--no-remote" => set_flag(&mut options.no_remote, argument)?,
            "--dry-run" => set_flag(&mut options.dry_run, argument)?,
            "--no-input" => set_flag(&mut options.no_input, argument)?,
            value => return Err(usage_error(&format!("unknown argument '{value}'"))),
        }
        index += 1;
    }
    if options.remote.is_some() && options.no_remote {
        return Err(usage_error(
            "--remote and --no-remote cannot be used together",
        ));
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

fn prompt_remote(
    i18n: &I18n,
    current: Option<&str>,
    reconfigure: bool,
) -> Result<RemoteSelection, String> {
    let message = if reconfigure {
        i18n.format(
            "init.remote_reconfigure_prompt",
            &[(
                "current",
                current.unwrap_or_else(|| i18n.text("common.none")),
            )],
        )
    } else {
        i18n.text("init.remote_prompt").to_string()
    };
    let answer = prompt(&message)?;
    if answer.is_empty() {
        Ok(RemoteSelection::Keep)
    } else if reconfigure && answer == "-" {
        Ok(RemoteSelection::Remove)
    } else {
        Ok(RemoteSelection::Set(answer))
    }
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

fn inspect_repository(paths: &AppPaths) -> Result<RepositoryState, String> {
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
        return Ok(RepositoryState {
            initialize: true,
            origin: None,
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
    Ok(RepositoryState {
        initialize: false,
        origin: existing,
    })
}

fn resolve_remote(
    existing: Option<String>,
    selection: RemoteSelection,
    reconfigure: bool,
) -> Result<(Option<String>, RemoteAction), String> {
    match selection {
        RemoteSelection::Keep => Ok((existing, RemoteAction::None)),
        RemoteSelection::Remove => {
            let action = if existing.is_some() {
                RemoteAction::Remove
            } else {
                RemoteAction::None
            };
            Ok((None, action))
        }
        RemoteSelection::Set(requested) => match existing {
            Some(existing) if existing == requested => Ok((Some(existing), RemoteAction::None)),
            Some(existing) if !reconfigure => Err(format!(
                "init: Git remote 'origin' is already set to '{existing}'"
            )),
            Some(_) => Ok((Some(requested), RemoteAction::SetUrl)),
            None => Ok((Some(requested), RemoteAction::Add)),
        },
    }
}

fn apply_repository(paths: &AppPaths, plan: &InitPlan) -> Result<(), String> {
    if plan.initialize_git {
        git(&paths.source_home, &["init"])?;
    }
    match plan.remote_action {
        RemoteAction::None => {}
        RemoteAction::Add => {
            let remote = plan
                .remote
                .as_deref()
                .ok_or_else(|| "init: missing remote for Git origin".to_string())?;
            git(&paths.source_home, &["remote", "add", "origin", remote])?;
        }
        RemoteAction::SetUrl => {
            let remote = plan
                .remote
                .as_deref()
                .ok_or_else(|| "init: missing replacement remote for Git origin".to_string())?;
            git(&paths.source_home, &["remote", "set-url", "origin", remote])?;
        }
        RemoteAction::Remove => {
            git(&paths.source_home, &["remote", "remove", "origin"])?;
        }
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
    let message = match (plan.reconfigure, dry_run) {
        (true, true) => "init.reconfigure_plan",
        (true, false) => "init.reconfiguring",
        (false, true) => "init.plan",
        (false, false) => "init.setting_up",
    };
    println!("{}", i18n.text(message));
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
    fn parses_reconfiguration_and_rejects_conflicting_remote_options() {
        let arguments = ["--reconfigure", "--no-remote", "--no-input"].map(OsString::from);
        let options = parse(&arguments).expect("reconfiguration should parse");
        assert!(options.reconfigure);
        assert!(options.no_remote);

        let conflicting = [
            "--reconfigure",
            "--remote",
            "replacement.git",
            "--no-remote",
        ]
        .map(OsString::from);
        let error = parse(&conflicting).expect_err("remote choices must be exclusive");
        assert!(error.contains("cannot be used together"));
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
    fn reconfigures_settings_and_origin_without_replacing_source() {
        let root = std::env::temp_dir().join(format!(
            "scv-init-reconfigure-test-{}",
            crate::storage::unique_nonce()
        ));
        let paths = AppPaths::isolated(root.clone()).expect("paths should resolve");
        let i18n = I18n::new(Locale::En).expect("catalog should load");
        let initial = [
            "--locale",
            "en",
            "--agent",
            "codex",
            "--remote",
            "initial.git",
            "--no-input",
        ]
        .map(OsString::from);
        run(&initial, &paths, &i18n).expect("initial setup should succeed");
        let original_manifest =
            fs::read(paths.source_manifest_path()).expect("manifest should be readable");
        let marker = paths.source_command_dir.join("preserve-me");
        fs::write(&marker, "user-owned").expect("marker should be written");

        let reconfigure = [
            "--reconfigure",
            "--locale",
            "ko",
            "--agent",
            "claude",
            "--remote",
            "replacement.git",
            "--no-input",
        ]
        .map(OsString::from);
        run(&reconfigure, &paths, &i18n).expect("reconfiguration should succeed");

        assert_eq!(
            fs::read(paths.source_manifest_path()).expect("manifest should remain readable"),
            original_manifest
        );
        assert_eq!(
            fs::read_to_string(marker).expect("marker should remain readable"),
            "user-owned"
        );
        let settings = Settings::load(&paths).expect("settings should load");
        assert_eq!(settings.ui.locale, Locale::Ko);
        assert_eq!(settings.create.agent.as_deref(), Some("claude"));
        let output = Command::new("git")
            .arg("-C")
            .arg(&paths.source_home)
            .args(["remote", "get-url", "origin"])
            .output()
            .expect("Git should start");
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "replacement.git"
        );
        fs::remove_dir_all(root).expect("fixture should be removed");
    }

    #[test]
    fn reconfiguration_can_remove_origin_and_repair_missing_git_metadata() {
        let root = std::env::temp_dir().join(format!(
            "scv-init-repair-test-{}",
            crate::storage::unique_nonce()
        ));
        let paths = AppPaths::isolated(root.clone()).expect("paths should resolve");
        crate::source::ensure_initialized(&paths).expect("source should initialize");
        let arguments = [
            "--reconfigure",
            "--locale",
            "en",
            "--agent",
            "agy",
            "--remote",
            "initial.git",
            "--no-input",
        ]
        .map(OsString::from);
        let i18n = I18n::new(Locale::En).expect("catalog should load");

        run(&arguments, &paths, &i18n).expect("reconfiguration should repair Git metadata");
        assert!(paths.source_home.join(".git").exists());
        let remove = ["--reconfigure", "--no-remote", "--no-input"].map(OsString::from);
        run(&remove, &paths, &i18n).expect("origin removal should succeed");
        let output = Command::new("git")
            .arg("-C")
            .arg(&paths.source_home)
            .args(["remote"])
            .output()
            .expect("Git should start");
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).trim().is_empty());
        fs::remove_dir_all(root).expect("fixture should be removed");
    }

    #[test]
    fn reconfigure_dry_run_preserves_settings_and_origin() {
        let root = std::env::temp_dir().join(format!(
            "scv-init-reconfigure-dry-run-test-{}",
            crate::storage::unique_nonce()
        ));
        let paths = AppPaths::isolated(root.clone()).expect("paths should resolve");
        let i18n = I18n::new(Locale::En).expect("catalog should load");
        let initial = [
            "--locale",
            "en",
            "--agent",
            "codex",
            "--remote",
            "initial.git",
            "--no-input",
        ]
        .map(OsString::from);
        run(&initial, &paths, &i18n).expect("initial setup should succeed");
        let config = fs::read(paths.config_path()).expect("config should be readable");

        let dry_run = [
            "--reconfigure",
            "--locale",
            "ko",
            "--agent",
            "agy",
            "--remote",
            "replacement.git",
            "--dry-run",
            "--no-input",
        ]
        .map(OsString::from);
        run(&dry_run, &paths, &i18n).expect("reconfiguration dry-run should succeed");

        assert_eq!(
            fs::read(paths.config_path()).expect("config should remain readable"),
            config
        );
        let output = Command::new("git")
            .arg("-C")
            .arg(&paths.source_home)
            .args(["remote", "get-url", "origin"])
            .output()
            .expect("Git should start");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "initial.git"
        );
        fs::remove_dir_all(root).expect("fixture should be removed");
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
