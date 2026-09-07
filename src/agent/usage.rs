use std::io::{self, BufRead, BufReader, Read};

use serde_json::Value;

const MAX_EVENT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub total: u64,
}

impl TokenUsage {
    fn new(input: u64, output: u64) -> Option<Self> {
        Some(Self {
            input,
            output,
            total: input.checked_add(output)?,
        })
    }

    fn add(self, other: Self) -> Option<Self> {
        Self::new(
            self.input.checked_add(other.input)?,
            self.output.checked_add(other.output)?,
        )
    }
}

#[derive(Clone, Copy)]
pub enum UsageFormat {
    Codex,
    Claude,
    Agy,
}

#[derive(Default)]
pub(super) struct UsageReport {
    pub tokens: Option<TokenUsage>,
    pub failed: bool,
}

struct Collector {
    format: UsageFormat,
    report: UsageReport,
    incomplete_turn: bool,
}

impl Collector {
    fn event(&mut self, line: &[u8]) {
        let Ok(event) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        match self.format {
            UsageFormat::Codex => match event["type"].as_str() {
                Some("turn.completed") => {
                    // Codex reports per-turn totals, including all tool steps.
                    // Cached input and reasoning output are already included.
                    let tokens = counts(&event["usage"], "input_tokens", "output_tokens");
                    self.report.tokens = match (self.report.tokens, tokens) {
                        (Some(previous), Some(current)) => previous.add(current),
                        (_, tokens) => tokens,
                    };
                    self.incomplete_turn |= self.report.tokens.is_none();
                }
                Some("turn.failed") => self.report.failed = true,
                _ => {}
            },
            UsageFormat::Claude
                if event["type"] == "result" && event["parent_tool_use_id"].is_null() =>
            {
                // Final totals replace snapshots; never add assistant/tool usage.
                self.report.failed |= event["is_error"].as_bool() == Some(true);
                self.report.tokens = claude_usage(&event);
            }
            UsageFormat::Agy if event["event"] == "result" => {
                let result = &event["result"];
                self.report.failed |= result["status"].as_str().is_some_and(|s| s != "SUCCESS");
                let usage = &result["usage"];
                // Thinking and cache reads are subsets of output/input totals.
                self.report.tokens = counts(usage, "input_tokens", "output_tokens")
                    .filter(|tokens| usage["total_tokens"].as_u64() == Some(tokens.total));
            }
            _ => {}
        }
    }
}

fn counts(usage: &Value, input: &str, output: &str) -> Option<TokenUsage> {
    TokenUsage::new(usage[input].as_u64()?, usage[output].as_u64()?)
}

fn optional_count(usage: &Value, key: &str) -> Option<u64> {
    match usage.get(key) {
        None => Some(0),
        Some(value) => value.as_u64(),
    }
}

fn claude_counts(usage: &Value, keys: [&str; 4]) -> Option<TokenUsage> {
    let input = usage[keys[0]]
        .as_u64()?
        .checked_add(optional_count(usage, keys[2])?)?
        .checked_add(optional_count(usage, keys[3])?)?;
    TokenUsage::new(input, usage[keys[1]].as_u64()?)
}

fn claude_usage(event: &Value) -> Option<TokenUsage> {
    // Per-model totals also account for models used by subagents when reported.
    if let Some(models) = event["modelUsage"].as_object().filter(|m| !m.is_empty()) {
        let mut total = TokenUsage::new(0, 0)?;
        for usage in models.values() {
            total = total.add(claude_counts(
                usage,
                [
                    "inputTokens",
                    "outputTokens",
                    "cacheReadInputTokens",
                    "cacheCreationInputTokens",
                ],
            )?)?;
        }
        Some(total)
    } else {
        claude_counts(
            &event["usage"],
            [
                "input_tokens",
                "output_tokens",
                "cache_read_input_tokens",
                "cache_creation_input_tokens",
            ],
        )
    }
}

/// Drain stdout concurrently with stdin delivery. Keep at most one bounded JSON
/// line, discard oversized events through their newline, and retain only numbers
/// and failure state. Provider text is never logged or persisted.
pub(super) fn read_usage(reader: impl Read, format: UsageFormat) -> io::Result<UsageReport> {
    let mut reader = BufReader::new(reader);
    let mut collector = Collector {
        format,
        report: UsageReport::default(),
        incomplete_turn: false,
    };
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            if !oversized && !line.is_empty() {
                collector.event(&line);
            }
            break;
        }
        let newline = chunk.iter().position(|byte| *byte == b'\n');
        let length = newline.map_or(chunk.len(), |position| position + 1);
        if !oversized {
            if line.len() + length > MAX_EVENT_BYTES {
                oversized = true;
                line.clear();
            } else {
                line.extend_from_slice(&chunk[..length]);
            }
        }
        reader.consume(length);
        if newline.is_some() {
            if !oversized {
                collector.event(&line);
            }
            line.clear();
            oversized = false;
        }
    }
    if collector.incomplete_turn {
        collector.report.tokens = None;
    }
    Ok(collector.report)
}

#[cfg(test)]
mod tests {
    use super::{MAX_EVENT_BYTES, TokenUsage, UsageFormat, read_usage};

    fn tokens(stream: &str, format: UsageFormat) -> Option<TokenUsage> {
        read_usage(stream.as_bytes(), format).unwrap().tokens
    }

    #[test]
    fn codex_sums_turns_without_double_counting_cache_reasoning_or_items() {
        let stream = r#"{"type":"item.completed","usage":{"input_tokens":99999,"output_tokens":99999}}
{"type":"turn.completed","usage":{"input_tokens":1000,"cached_input_tokens":900,"output_tokens":80,"reasoning_output_tokens":50}}
{"type":"turn.completed","usage":{"input_tokens":2000,"output_tokens":20}}
"#;
        assert_eq!(
            tokens(stream, UsageFormat::Codex),
            TokenUsage::new(3000, 100)
        );
    }

    #[test]
    fn claude_uses_final_totals_including_cache_without_adding_snapshots() {
        let stream = r#"{"type":"assistant","message":{"usage":{"input_tokens":99999,"output_tokens":99999}}}
{"type":"result","usage":{"input_tokens":1,"output_tokens":1}}
{"type":"result","usage":{"input_tokens":10,"cache_read_input_tokens":100,"cache_creation_input_tokens":20,"output_tokens":40}}
{"type":"result","parent_tool_use_id":"child","usage":{"input_tokens":99999,"output_tokens":99999}}
"#;
        assert_eq!(
            tokens(stream, UsageFormat::Claude),
            TokenUsage::new(130, 40)
        );
        let models = r#"{"type":"result","usage":{"input_tokens":1,"output_tokens":1},"modelUsage":{"main":{"inputTokens":10,"cacheReadInputTokens":100,"cacheCreationInputTokens":20,"outputTokens":40},"helper":{"inputTokens":5,"outputTokens":10}}}"#;
        assert_eq!(
            tokens(models, UsageFormat::Claude),
            TokenUsage::new(135, 50)
        );
    }

    #[test]
    fn agy_uses_result_totals_without_adding_steps_thinking_or_cache() {
        let stream = r#"{"event":"step_update","step_update":{"usage":{"input_tokens":100,"output_tokens":80}}}
{"event":"result","result":{"status":"SUCCESS","usage":{"input_tokens":1000,"output_tokens":100,"cache_read_tokens":800,"thinking_tokens":80,"total_tokens":1100}}}"#;
        assert_eq!(tokens(stream, UsageFormat::Agy), TokenUsage::new(1000, 100));
    }

    #[test]
    fn ignores_large_or_malformed_transcripts_and_handles_unterminated_final_line() {
        let mut stream = vec![b'x'; MAX_EVENT_BYTES + 8192];
        stream.extend_from_slice(b"\n\xff invalid json\n");
        stream.extend_from_slice(
            br#"{"type":"turn.completed","usage":{"input_tokens":0,"output_tokens":0}}"#,
        );
        let report = read_usage(&stream[..], UsageFormat::Codex).unwrap();
        assert_eq!(report.tokens, TokenUsage::new(0, 0));
        assert!(!report.failed);
    }

    #[test]
    fn missing_invalid_and_overflowing_counts_are_unavailable_not_zero() {
        for usage in [
            "null",
            "{}",
            r#"{"input_tokens":-1,"output_tokens":1}"#,
            r#"{"input_tokens":"HIDDEN","output_tokens":1}"#,
            r#"{"input_tokens":0.5,"output_tokens":1}"#,
            r#"{"input_tokens":18446744073709551615,"output_tokens":1}"#,
        ] {
            let stream = format!(r#"{{"type":"turn.completed","usage":{usage}}}"#);
            assert_eq!(tokens(&stream, UsageFormat::Codex), None);
            let stream = format!(r#"{{"type":"result","usage":{usage}}}"#);
            assert_eq!(tokens(&stream, UsageFormat::Claude), None);
        }
        let missing_turn = "{\"type\":\"turn.completed\"}\n{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}";
        assert_eq!(tokens(missing_turn, UsageFormat::Codex), None);
        let inconsistent = r#"{"event":"result","result":{"usage":{"input_tokens":100,"output_tokens":20,"total_tokens":999}}}"#;
        assert_eq!(tokens(inconsistent, UsageFormat::Agy), None);
    }
}
