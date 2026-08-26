use std::ffi::OsString;

use crate::help;
use crate::metadata::Registry;

use super::write_json;

pub fn run(arguments: &[OsString], registry: &Registry) -> Result<i32, String> {
    let mut name = None;
    let mut json = false;
    for argument in arguments {
        let argument = argument
            .to_str()
            .ok_or_else(|| "info: arguments must be valid UTF-8".to_string())?;
        match argument {
            "--json" if !json => json = true,
            value if value.starts_with('-') => {
                return Err(format!("info: unknown or duplicate option '{value}'"));
            }
            value if name.is_none() => name = Some(value),
            value => return Err(format!("info: unexpected argument '{value}'")),
        }
    }
    let name = name.ok_or_else(|| "info: missing required command name".to_string())?;
    let command = registry
        .get(name)
        .ok_or_else(|| format!("info: unknown command '{name}'"))?;
    if json {
        write_json("info", command)?;
    } else {
        help::print_info(command);
    }
    Ok(0)
}
