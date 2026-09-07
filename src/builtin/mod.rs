mod add;
mod apply;
mod config;
mod create;
mod history;
mod info;
mod init;
mod list;
mod paths;
mod remove;
mod rollback;
mod status;
mod sync;

use std::ffi::OsString;
use std::io::{self, Write};

use serde::Serialize;

use crate::i18n::I18n;
use crate::metadata::Registry;
use crate::paths::AppPaths;

pub fn run(
    command: &str,
    arguments: &[OsString],
    paths: &AppPaths,
    registry: &Registry,
    i18n: &I18n,
) -> Result<i32, String> {
    match command {
        "add" => add::run(arguments, paths, registry),
        "apply" => apply::run(arguments, paths),
        "config" => config::run(arguments, paths, i18n),
        "create" => create::run(arguments, paths, registry, i18n),
        "history" => history::run(arguments, paths, i18n),
        "rm" => remove::run(arguments, paths, registry),
        "list" => list::run(arguments, registry, i18n),
        "info" => info::run(arguments, registry, i18n),
        "init" => init::run(arguments, paths, i18n),
        "paths" => paths::run(arguments, paths),
        "rollback" => rollback::run(arguments, paths),
        "status" => status::run(arguments, paths),
        "sync" => sync::run(arguments, paths),
        _ => Err(i18n.format("builtin.not_implemented", &[("command", command)])),
    }
}

fn write_json<T: Serialize + ?Sized>(command: &str, value: &T) -> Result<(), String> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer_pretty(&mut output, value)
        .map_err(|error| format!("{command}: could not serialize JSON: {error}"))?;
    writeln!(output).map_err(|error| format!("{command}: could not write JSON: {error}"))
}
