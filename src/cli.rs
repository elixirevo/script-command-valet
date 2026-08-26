use std::env;
use std::ffi::OsString;

use crate::builtin;
use crate::command;
use crate::help;
use crate::metadata::Registry;
use crate::paths::AppPaths;

pub fn run() -> Result<i32, String> {
    let paths = AppPaths::discover()?;
    let mut arguments: Vec<OsString> = env::args_os().skip(1).collect();

    if arguments
        .first()
        .and_then(|argument| argument.to_str())
        .is_some_and(|command| matches!(command, "-V" | "--version"))
    {
        println!("scv {}", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }

    let requested = arguments
        .first()
        .and_then(|argument| argument.to_str())
        .unwrap_or("");
    let registry = match Registry::load(&paths) {
        Ok(registry) => registry,
        Err(error) if recovery_builtin(requested) => {
            eprintln!(
                "scv: warning: current activation could not be loaded; running recovery builtin '{requested}': {error}"
            );
            Registry::builtins()?
        }
        Err(error) => {
            return Err(format!(
                "current activation could not be loaded: {error}. Run 'scv apply' or 'scv rollback'"
            ));
        }
    };

    if arguments.is_empty() {
        help::print_top(&registry);
        return Ok(0);
    }

    let command = arguments.remove(0);
    let command = command
        .to_str()
        .ok_or_else(|| "command name must be valid UTF-8".to_string())?;

    if matches!(command, "-h" | "--help" | "help") {
        help::print_top(&registry);
        return Ok(0);
    }
    let metadata = registry
        .get(command)
        .ok_or_else(|| format!("unknown command '{command}'. Run 'scv --help'"))?;

    if requests_help(&arguments) {
        help::print_command(metadata);
        return Ok(0);
    }

    if metadata.builtin {
        builtin::run(command, &arguments, &paths, &registry)
    } else {
        command::run_external(metadata, &arguments, &paths)
    }
}

fn recovery_builtin(command: &str) -> bool {
    matches!(
        command,
        "add" | "apply" | "config" | "create" | "paths" | "rm" | "rollback" | "status" | "sync"
    )
}

fn requests_help(arguments: &[OsString]) -> bool {
    for argument in arguments {
        match argument.to_str() {
            Some("--") => return false,
            Some("-h" | "--help") => return true,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{recovery_builtin, requests_help};

    #[test]
    fn detects_help_before_the_option_terminator() {
        let arguments = ["target", "--help"].map(OsString::from);
        assert!(requests_help(&arguments));

        let arguments = ["--", "--help"].map(OsString::from);
        assert!(!requests_help(&arguments));
    }

    #[test]
    fn limits_recovery_mode_to_management_builtins() {
        assert!(recovery_builtin("apply"));
        assert!(recovery_builtin("rollback"));
        assert!(!recovery_builtin("list"));
        assert!(!recovery_builtin("user-command"));
    }
}
