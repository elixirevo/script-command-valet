mod activation;
mod agent;
mod builtin;
mod cli;
mod command;
mod config;
mod generation;
mod help;
mod history;
mod i18n;
mod input;
mod metadata;
mod oneshot;
mod package;
mod paths;
mod source;
mod storage;
mod sync;

fn main() {
    match cli::run() {
        Ok(code) => std::process::exit(code),
        Err(message) => {
            eprintln!("scv: {message}");
            std::process::exit(1);
        }
    }
}
