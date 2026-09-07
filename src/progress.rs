use std::io::{self, IsTerminal, Write};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::agent::GenerationUsage;
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

    pub fn summary(elapsed: Duration, usage: GenerationUsage, i18n: &I18n) {
        write_line(&format_summary(elapsed, usage, i18n));
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

fn format_summary(elapsed: Duration, usage: GenerationUsage, i18n: &I18n) -> String {
    let tokens = match usage.tokens {
        Some(usage) => i18n.format(
            "generation.tokens",
            &[
                ("total", &group_digits(usage.total)),
                ("input", &group_digits(usage.input)),
                ("output", &group_digits(usage.output)),
            ],
        ),
        None => i18n.text("generation.tokens_unavailable").to_string(),
    };
    let summary = i18n.format(
        "generation.summary",
        &[
            ("seconds", &format!("{:.1}", elapsed.as_secs_f64())),
            ("tokens", &tokens),
        ],
    );
    let cache = match usage.tokens.and_then(|tokens| {
        tokens
            .cached_input
            .map(|cached| (cached, tokens.input - cached))
    }) {
        Some((cached, uncached)) => i18n.format(
            "generation.cache",
            &[
                ("cached", &group_digits(cached)),
                ("uncached", &group_digits(uncached)),
            ],
        ),
        None => i18n.text("generation.cache_unavailable").to_string(),
    };
    let tools = match usage.tool_calls {
        Some(count) => i18n.format("generation.tools", &[("count", &group_digits(count))]),
        None => i18n.text("generation.tools_unavailable").to_string(),
    };
    format!("{summary}\n{cache} · {tools}")
}

fn group_digits(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
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

#[cfg(test)]
mod tests {
    use super::format_summary;
    use crate::agent::{GenerationUsage, TokenUsage};
    use crate::i18n::{I18n, Locale};
    use std::time::Duration;

    #[test]
    fn formats_elapsed_time_and_distinguishes_zero_from_unknown_usage() {
        let elapsed = Duration::from_millis(12345);
        let en = I18n::new(Locale::En).unwrap();
        let ko = I18n::new(Locale::Ko).unwrap();
        let usage = GenerationUsage {
            tokens: Some(TokenUsage {
                input: 12000,
                output: 800,
                total: 12800,
                cached_input: Some(10000),
            }),
            tool_calls: Some(2),
        };
        assert_eq!(
            format_summary(elapsed, usage, &en),
            "Generation complete · 12.3s · Cumulative tokens 12,800 (input 12,000 / output 800)\nInput cache 10,000 / non-cached 2,000 · Tool calls 2"
        );
        assert_eq!(
            format_summary(elapsed, usage, &ko),
            "생성 완료 · 12.3초 · 누적 토큰 12,800 (입력 12,000 / 출력 800)\n입력 캐시 10,000 / 비캐시 2,000 · 도구 호출 2회"
        );
        let unknown = format_summary(elapsed, GenerationUsage::default(), &ko);
        assert!(unknown.contains("토큰 사용량 확인 불가"));
        assert!(unknown.contains("입력 캐시 내역 확인 불가"));
        assert!(unknown.contains("도구 호출 수 확인 불가"));
        let zero = GenerationUsage {
            tokens: Some(TokenUsage {
                input: 0,
                output: 0,
                total: 0,
                cached_input: Some(0),
            }),
            tool_calls: Some(0),
        };
        let zero = format_summary(elapsed, zero, &en);
        assert!(zero.contains("Cumulative tokens 0 (input 0 / output 0)"));
        assert!(zero.contains("Input cache 0 / non-cached 0 · Tool calls 0"));
    }
}
