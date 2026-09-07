use std::io::{self, IsTerminal, Write};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::i18n::I18n;

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// A generation stage owns its spinner so every return path stops it before
/// diagnostics or approval prompts are printed. Progress never writes to stdout.
pub struct GenerationProgress<'a> {
    label: String,
    i18n: &'a I18n,
    animation: Option<(Sender<()>, JoinHandle<()>)>,
    finished: bool,
}

impl<'a> GenerationProgress<'a> {
    pub fn start(step: u8, message: &str, i18n: &'a I18n) -> Self {
        let label = format!("[{step}/4] {message}");
        let animate = io::stderr().is_terminal()
            && std::env::var_os("TERM").is_none_or(|term| term != "dumb");
        let animation = if animate {
            let (stop, receiver) = mpsc::channel();
            write_frame(FRAMES[0], &label, false);
            let running_label = label.clone();
            let handle = thread::Builder::new()
                .name("scv-generation-progress".to_string())
                .spawn(move || {
                    let mut frame = 1;
                    while matches!(
                        receiver.recv_timeout(Duration::from_millis(80)),
                        Err(mpsc::RecvTimeoutError::Timeout)
                    ) {
                        write_frame(FRAMES[frame % FRAMES.len()], &running_label, false);
                        frame += 1;
                    }
                });
            match handle {
                Ok(handle) => Some((stop, handle)),
                Err(_) => {
                    write_frame("·", &label, true);
                    None
                }
            }
        } else {
            write_line(&label);
            None
        };
        Self {
            label,
            i18n,
            animation,
            finished: false,
        }
    }

    pub fn complete(mut self) {
        self.finish(true);
    }

    pub fn approval(message: &str) {
        write_line(&format!("[4/4] {message}"));
    }

    fn finish(&mut self, success: bool) {
        if self.finished {
            return;
        }
        self.finished = true;
        let status = self.i18n.text(if success {
            "generation.done"
        } else {
            "generation.failed"
        });
        let label = format!("{} — {status}", self.label);
        if let Some((stop, handle)) = self.animation.take() {
            let _ = stop.send(());
            let _ = handle.join();
            write_frame(if success { "✓" } else { "✗" }, &label, true);
        } else if !success {
            write_line(&label);
        }
    }
}

impl Drop for GenerationProgress<'_> {
    fn drop(&mut self) {
        self.finish(false);
    }
}

fn write_frame(marker: &str, label: &str, newline: bool) {
    let mut stderr = io::stderr().lock();
    // Only the fixed-width marker changes while running. No cursor visibility,
    // raw mode, or ANSI state needs restoring on interruption.
    let _ = write!(stderr, "\r{marker} {label}");
    if newline {
        let _ = writeln!(stderr);
    }
    let _ = stderr.flush();
}

fn write_line(message: &str) {
    let _ = writeln!(io::stderr().lock(), "{message}");
}
