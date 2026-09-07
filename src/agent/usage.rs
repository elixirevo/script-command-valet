use std::collections::HashSet;
use std::io::{self, BufRead, BufReader, Read};

use serde_json::Value;

const MAX_EVENT_BYTES: usize = 1024 * 1024;
const MAX_TOOL_IDS: usize = 4096;
const MAX_ID_BYTES: usize = 128;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GenerationUsage {
    pub tokens: Option<TokenUsage>,
    pub tool_calls: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenUsage {
    pub input: u64,
    pub output: u64,
    pub total: u64,
    /// Cache reads are a subset of input, not additional tokens.
    pub cached_input: Option<u64>,
}

impl TokenUsage {
    fn new(input: u64, output: u64) -> Option<Self> {
        Some(Self {
            input,
            output,
            total: input.checked_add(output)?,
            cached_input: None,
        })
    }

    fn with_cache(mut self, cached_input: Option<u64>) -> Self {
        self.cached_input = cached_input.filter(|cached| *cached <= self.input);
        self
    }

    fn add(self, other: Self) -> Option<Self> {
        Some(
            Self::new(
                self.input.checked_add(other.input)?,
                self.output.checked_add(other.output)?,
            )?
            .with_cache(
                self.cached_input
                    .zip(other.cached_input)
                    .and_then(|(a, b)| a.checked_add(b)),
            ),
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
    pub usage: GenerationUsage,
    pub failed: bool,
}

struct Collector {
    format: UsageFormat,
    report: UsageReport,
    incomplete_turn: bool,
    completed: bool,
    active_turn: bool,
    tools: Option<HashSet<(String, String)>>,
}

impl Collector {
    fn event(&mut self, line: &[u8]) {
        let Ok(event) = serde_json::from_slice::<Value>(line) else {
            self.tools = None;
            return;
        };
        self.tool_event(&event);
        match self.format {
            UsageFormat::Codex => match event["type"].as_str() {
                Some("turn.started") => self.active_turn = true,
                Some("turn.completed") => {
                    self.active_turn = false;
                    self.completed = true;
                    // Codex reports per-turn totals, including all tool steps.
                    // Cached input and reasoning output are already included.
                    let tokens =
                        counts(&event["usage"], "input_tokens", "output_tokens").map(|tokens| {
                            tokens.with_cache(event["usage"]["cached_input_tokens"].as_u64())
                        });
                    self.report.usage.tokens = match (self.report.usage.tokens, tokens) {
                        (Some(previous), Some(current)) => previous.add(current),
                        (_, tokens) => tokens,
                    };
                    self.incomplete_turn |= self.report.usage.tokens.is_none();
                }
                Some("turn.failed") => {
                    self.active_turn = false;
                    self.report.failed = true;
                }
                _ => {}
            },
            UsageFormat::Claude
                if event["type"] == "result" && event["parent_tool_use_id"].is_null() =>
            {
                self.completed = true;
                // Final totals replace snapshots; never add assistant/tool usage.
                self.report.failed |= event["is_error"].as_bool() == Some(true);
                self.report.usage.tokens = claude_usage(&event);
            }
            UsageFormat::Agy if event["event"] == "result" => {
                self.completed = true;
                let result = &event["result"];
                self.report.failed |= result["status"].as_str().is_some_and(|s| s != "SUCCESS");
                let usage = &result["usage"];
                // Keep the final reported totals. Agy does not define the cache
                // subset consistently enough to derive a non-cached input count.
                self.report.usage.tokens = counts(usage, "input_tokens", "output_tokens")
                    .filter(|tokens| usage["total_tokens"].as_u64() == Some(tokens.total));
            }
            _ => {}
        }
    }

    fn tool_event(&mut self, event: &Value) {
        match self.format {
            UsageFormat::Codex if event["type"] == "item.completed" => {
                let item = &event["item"];
                match item["type"].as_str() {
                    Some("command_execution" | "file_change" | "mcp_tool_call" | "web_search") => {
                        self.tool_id("codex", item["id"].as_str())
                    }
                    Some("agent_message" | "reasoning" | "error" | "plan") => {}
                    _ => self.tools = None,
                }
            }
            UsageFormat::Claude if event["type"] == "assistant" => {
                if let Some(content) = event["message"]["content"].as_array() {
                    for block in content {
                        match block["type"].as_str() {
                            Some("tool_use") => self.tool_id("claude", block["id"].as_str()),
                            Some("text" | "thinking" | "redacted_thinking") => {}
                            _ => self.tools = None,
                        }
                    }
                } else {
                    self.tools = None;
                }
            }
            UsageFormat::Agy if event["event"] == "step_update" => {
                let step = &event["step_update"];
                match (step["step_type"].as_str(), step["state"].as_str()) {
                    (Some("tool"), Some("DONE")) => {
                        if let (Some(conversation), Some(index)) = (
                            step["conversation_id"].as_str(),
                            step["step_index"].as_u64(),
                        ) {
                            self.tool_id(conversation, Some(&index.to_string()));
                        } else {
                            self.tools = None;
                        }
                    }
                    (
                        Some("tool" | "user_input" | "agent_response" | "checkpoint"),
                        Some("ACTIVE" | "DONE"),
                    ) => {}
                    _ => self.tools = None,
                }
            }
            _ => {}
        }
    }

    fn tool_id(&mut self, scope: &str, id: Option<&str>) {
        let Some(id) = id
            .filter(|id| !id.is_empty() && id.len() <= MAX_ID_BYTES)
            .filter(|_| !scope.is_empty() && scope.len() <= MAX_ID_BYTES)
        else {
            self.tools = None;
            return;
        };
        if let Some(tools) = &mut self.tools {
            let key = (scope.to_string(), id.to_string());
            if tools.contains(&key) {
                return;
            }
            if tools.len() == MAX_TOOL_IDS {
                self.tools = None;
            } else {
                tools.insert(key);
            }
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
    Some(TokenUsage::new(input, usage[keys[1]].as_u64()?)?.with_cache(usage[keys[2]].as_u64()))
}

fn claude_usage(event: &Value) -> Option<TokenUsage> {
    // Per-model totals also account for models used by subagents when reported.
    if let Some(models) = event["modelUsage"].as_object().filter(|m| !m.is_empty()) {
        let mut total = TokenUsage::new(0, 0)?.with_cache(Some(0));
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
/// line, discard oversized events through their newline, and retain only numbers,
/// failure state, and bounded identifiers for deduplication. Provider text is never
/// logged or persisted.
pub(super) fn read_usage(reader: impl Read, format: UsageFormat) -> io::Result<UsageReport> {
    let mut reader = BufReader::new(reader);
    let mut collector = Collector {
        format,
        report: UsageReport::default(),
        incomplete_turn: false,
        completed: false,
        active_turn: false,
        tools: Some(HashSet::new()),
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
                collector.tools = None;
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
    if collector.incomplete_turn || collector.active_turn {
        collector.report.usage.tokens = None;
    }
    if collector.completed && !collector.active_turn {
        collector.report.usage.tool_calls = collector.tools.map(|tools| tools.len() as u64);
    }
    Ok(collector.report)
}

#[cfg(test)]
mod tests {
    use super::{MAX_EVENT_BYTES, TokenUsage, UsageFormat, read_usage};

    fn tokens(stream: &str, format: UsageFormat) -> Option<TokenUsage> {
        read_usage(stream.as_bytes(), format).unwrap().usage.tokens
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
            TokenUsage::new(130, 40).map(|tokens| tokens.with_cache(Some(100)))
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
        assert_eq!(report.usage.tokens, TokenUsage::new(0, 0));
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

    #[test]
    fn cache_breakdown_is_a_validated_subset_and_requires_every_turn() {
        for (cache, expected) in [
            ("0", Some(0)),
            ("900", Some(900)),
            ("1001", None),
            ("null", None),
            ("-1", None),
            ("\"900\"", None),
        ] {
            let stream = format!(
                r#"{{"type":"turn.completed","usage":{{"input_tokens":1000,"output_tokens":20,"cached_input_tokens":{cache}}}}}"#
            );
            let usage = tokens(&stream, UsageFormat::Codex).unwrap();
            assert_eq!(usage.input, 1000);
            assert_eq!(usage.cached_input, expected);
        }
        let stream = "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":1000,\"output_tokens\":20,\"cached_input_tokens\":900}}\n";
        assert_eq!(
            tokens(&stream.repeat(2), UsageFormat::Codex)
                .unwrap()
                .cached_input,
            Some(1800)
        );
        let missing =
            r#"{"type":"turn.completed","usage":{"input_tokens":1000,"output_tokens":20}}"#;
        assert_eq!(
            tokens(&format!("{stream}{missing}"), UsageFormat::Codex)
                .unwrap()
                .cached_input,
            None
        );
    }

    #[test]
    fn counts_distinct_codex_tool_operations_and_never_counts_updates_or_reasoning() {
        let stream = r#"{"type":"turn.started"}
{"type":"item.started","item":{"id":"1","type":"command_execution"}}
{"type":"item.updated","item":{"id":"1","type":"command_execution"}}
{"type":"item.completed","item":{"id":"1","type":"command_execution","status":"failed"}}
{"type":"item.completed","item":{"id":"1","type":"command_execution"}}
{"type":"item.completed","item":{"id":"2","type":"file_change"}}
{"type":"item.completed","item":{"id":"3","type":"mcp_tool_call"}}
{"type":"item.completed","item":{"id":"4","type":"web_search"}}
{"type":"item.completed","item":{"id":"5","type":"reasoning"}}
{"type":"item.completed","item":{"id":"6","type":"agent_message"}}
{"type":"item.completed","item":{"id":"7","type":"plan"}}
{"type":"turn.completed","usage":{"input_tokens":1000,"output_tokens":20}}
"#;
        let usage = read_usage(stream.as_bytes(), UsageFormat::Codex)
            .unwrap()
            .usage;
        assert_eq!(usage.tool_calls, Some(4));
        assert_eq!(usage.tokens.unwrap().input, 1000);
    }

    #[test]
    fn counts_claude_tool_blocks_and_agy_done_steps_by_their_own_ids() {
        let claude = r#"{"type":"assistant","message":{"id":"shared","content":[{"type":"tool_use","id":"a"},{"type":"tool_use","id":"b"},{"type":"text","text":"ignored"}]}}
{"type":"assistant","message":{"id":"shared","content":[{"type":"tool_use","id":"a"}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"a"}]}}
{"type":"result","usage":{"input_tokens":10,"cache_creation_input_tokens":20,"cache_read_input_tokens":70,"output_tokens":5}}
"#;
        let usage = read_usage(claude.as_bytes(), UsageFormat::Claude)
            .unwrap()
            .usage;
        assert_eq!(usage.tool_calls, Some(2));
        assert_eq!(usage.tokens.unwrap().cached_input, Some(70));
        assert_eq!(usage.tokens.unwrap().input, 100);
        let malformed = claude.replace("\"type\":\"text\"", "\"type\":\"unknown\"");
        assert_eq!(
            read_usage(malformed.as_bytes(), UsageFormat::Claude)
                .unwrap()
                .usage
                .tool_calls,
            None
        );
        let agy = r#"{"event":"step_update","step_update":{"conversation_id":"a","step_index":1,"step_type":"tool","state":"ACTIVE"}}
{"event":"step_update","step_update":{"conversation_id":"a","step_index":1,"step_type":"tool","state":"DONE"}}
{"event":"step_update","step_update":{"conversation_id":"a","step_index":1,"step_type":"tool","state":"DONE"}}
{"event":"step_update","step_update":{"conversation_id":"b","step_index":1,"step_type":"tool","state":"DONE"}}
{"event":"result","result":{"status":"SUCCESS","usage":{"input_tokens":10,"cache_read_tokens":5,"output_tokens":5,"total_tokens":15}}}
"#;
        let usage = read_usage(agy.as_bytes(), UsageFormat::Agy).unwrap().usage;
        assert_eq!(usage.tool_calls, Some(2));
        assert_eq!(usage.tokens.unwrap().cached_input, None);
        for malformed in [
            agy.replace("\"tool\"", "\"unknown\""),
            agy.replace("\"DONE\"", "null"),
        ] {
            assert_eq!(
                read_usage(malformed.as_bytes(), UsageFormat::Agy)
                    .unwrap()
                    .usage
                    .tool_calls,
                None
            );
        }
    }

    #[test]
    fn distinguishes_zero_tools_from_incomplete_or_damaged_streams() {
        let done = r#"{"type":"turn.completed","usage":{"input_tokens":1000,"output_tokens":20}}"#;
        let read = |stream: &str| {
            read_usage(stream.as_bytes(), UsageFormat::Codex)
                .unwrap()
                .usage
        };
        assert_eq!(read(done).tool_calls, Some(0));
        assert_eq!(read("").tool_calls, None);
        for damaged in [
            "invalid json".to_string(),
            "x".repeat(MAX_EVENT_BYTES + 1),
            r#"{"type":"item.completed","item":{"type":"command_execution"}}"#.to_string(),
        ] {
            let usage = read(&format!("{damaged}\n{done}"));
            assert_eq!(usage.tool_calls, None);
            assert_eq!(usage.tokens.unwrap().input, 1000);
        }
        let partial = read(&format!("{done}\n{{\"type\":\"turn.started\"}}"));
        assert_eq!(partial.tool_calls, None);
        assert_eq!(partial.tokens, None);
    }

    #[test]
    fn bounds_tool_identifier_memory_without_losing_terminal_tokens() {
        let mut stream = String::new();
        for id in 0..=super::MAX_TOOL_IDS {
            stream.push_str(&format!("{{\"type\":\"item.completed\",\"item\":{{\"type\":\"file_change\",\"id\":\"{id}\"}}}}\n"));
        }
        stream
            .push_str(r#"{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}"#);
        let usage = read_usage(stream.as_bytes(), UsageFormat::Codex)
            .unwrap()
            .usage;
        assert_eq!(usage.tool_calls, None);
        assert_eq!(usage.tokens.unwrap().total, 2);
    }
}
