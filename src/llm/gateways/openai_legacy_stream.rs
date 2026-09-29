//! OpenAI-compatible server-sent events for the legacy `complete_stream` API.
//!
//! Content deltas become [`StreamChunk::Content`]. Tool call fragments are
//! accumulated by index and yielded as one [`StreamChunk::ToolCalls`] when the
//! finish reason is `tool_calls`, or at `data: [DONE]`. Frames that are not
//! JSON are logged and skipped.

use crate::error::Result;
use crate::llm::gateway::StreamChunk;
use crate::llm::models::LlmToolCall;
use futures::stream::{Stream, StreamExt};
use serde_json::Value;
use std::collections::HashMap;
use tracing::warn;

/// One meaningful `data:` line of a legacy stream.
pub(crate) enum LegacyLine {
    /// The `data: [DONE]` marker.
    Done,
    /// A JSON chunk.
    Frame(Value),
}

/// Read one SSE line. Blank lines, comments, other fields and frames that are
/// not JSON carry nothing.
pub(crate) fn read_legacy_line(line: &str) -> Option<LegacyLine> {
    let data = line.trim().strip_prefix("data: ")?;
    if data == "[DONE]" {
        return Some(LegacyLine::Done);
    }
    match serde_json::from_str::<Value>(data) {
        Ok(frame) => Some(LegacyLine::Frame(frame)),
        Err(e) => {
            warn!("Failed to parse streaming chunk: {}", e);
            None
        }
    }
}

/// Turns legacy stream frames into chunks, accumulating tool calls.
#[derive(Default)]
pub(crate) struct OpenAiLegacyParser {
    tool_calls: HashMap<usize, ToolCallAccumulator>,
}

impl OpenAiLegacyParser {
    /// Chunks for one SSE line.
    pub(crate) fn parse_line(&mut self, line: &str) -> Vec<StreamChunk> {
        match read_legacy_line(line) {
            Some(LegacyLine::Done) => self.done(),
            Some(LegacyLine::Frame(frame)) => self.frame(&frame),
            None => Vec::new(),
        }
    }

    /// Chunks for the `data: [DONE]` marker: any tool calls still pending.
    pub(crate) fn done(&self) -> Vec<StreamChunk> {
        self.complete_tool_calls().into_iter().collect()
    }

    /// Chunks for one JSON frame.
    pub(crate) fn frame(&mut self, frame: &Value) -> Vec<StreamChunk> {
        let Some(choice) = frame["choices"].as_array().and_then(|choices| choices.first()) else {
            return Vec::new();
        };
        let delta = &choice["delta"];
        let mut chunks = Vec::new();

        if let Some(content) = delta["content"].as_str() {
            if !content.is_empty() {
                chunks.push(StreamChunk::Content(content.to_string()));
            }
        }

        if let Some(tool_calls) = delta["tool_calls"].as_array() {
            for tc in tool_calls {
                self.accumulate(tc);
            }
        }

        if choice["finish_reason"].as_str() == Some("tool_calls") && !self.tool_calls.is_empty() {
            chunks.extend(self.complete_tool_calls());
            self.tool_calls.clear();
        }

        chunks
    }

    fn accumulate(&mut self, tc: &Value) {
        let Some(index) = tc["index"].as_u64() else {
            return;
        };
        let acc = self.tool_calls.entry(index as usize).or_default();

        // The first fragment carries the id and function name.
        if let Some(id) = tc["id"].as_str() {
            acc.id = Some(id.to_string());
        }
        if let Some(name) = tc["function"]["name"].as_str() {
            acc.name = Some(name.to_string());
        }
        // Any fragment may carry part of the arguments.
        if let Some(args) = tc["function"]["arguments"].as_str() {
            acc.arguments.push_str(args);
        }
    }

    /// A tool-calls chunk for the accumulated calls, if any are complete.
    fn complete_tool_calls(&self) -> Option<StreamChunk> {
        let calls = build_complete_tool_calls(&self.tool_calls);
        (!calls.is_empty()).then_some(StreamChunk::ToolCalls(calls))
    }
}

/// Read a legacy SSE response body, handing each complete line to `parse_line`.
///
/// A transport error ends the stream with that error.
pub(crate) fn legacy_body_stream<'a, S, B, F>(
    bytes: S,
    mut parse_line: F,
) -> impl Stream<Item = Result<StreamChunk>> + Send + 'a
where
    S: Stream<Item = std::result::Result<B, reqwest::Error>> + Send + 'a,
    B: AsRef<[u8]> + Send + 'a,
    F: FnMut(&str) -> Vec<StreamChunk> + Send + 'a,
{
    async_stream::stream! {
        let mut bytes = Box::pin(bytes);
        let mut buffer = String::new();

        while let Some(chunk_result) = bytes.next().await {
            match chunk_result {
                Ok(chunk) => {
                    if let Ok(text) = std::str::from_utf8(chunk.as_ref()) {
                        buffer.push_str(text);

                        while let Some(line_end) = buffer.find('\n') {
                            let line = buffer[..line_end].to_string();
                            buffer = buffer[line_end + 1..].to_string();

                            for chunk in parse_line(&line) {
                                yield Ok(chunk);
                            }
                        }
                    }
                }
                Err(e) => {
                    yield Err(e.into());
                    return;
                }
            }
        }
    }
}

/// Accumulator for one streamed tool call.
#[derive(Default)]
struct ToolCallAccumulator {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

/// Build complete tool calls from accumulators, in index order.
///
/// A call whose name never arrived is dropped.
fn build_complete_tool_calls(
    accumulators: &HashMap<usize, ToolCallAccumulator>,
) -> Vec<LlmToolCall> {
    let mut indices: Vec<_> = accumulators.keys().collect();
    indices.sort();

    indices
        .iter()
        .filter_map(|&&index| {
            let acc = accumulators.get(&index)?;
            let name = acc.name.clone()?;

            let arguments: HashMap<String, Value> =
                serde_json::from_str(&acc.arguments).unwrap_or_default();

            Some(LlmToolCall {
                id: acc.id.clone(),
                name,
                arguments,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(lines: &[&str]) -> Vec<StreamChunk> {
        let mut parser = OpenAiLegacyParser::default();
        lines.iter().flat_map(|line| parser.parse_line(line)).collect()
    }

    #[test]
    fn test_build_complete_tool_calls() {
        let mut accumulators = HashMap::new();
        accumulators.insert(
            0,
            ToolCallAccumulator {
                id: Some("call_123".to_string()),
                name: Some("get_weather".to_string()),
                arguments: r#"{"location": "NYC"}"#.to_string(),
            },
        );
        accumulators.insert(
            1,
            ToolCallAccumulator {
                id: Some("call_456".to_string()),
                name: Some("search".to_string()),
                arguments: r#"{"query": "test"}"#.to_string(),
            },
        );

        let result = build_complete_tool_calls(&accumulators);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, Some("call_123".to_string()));
        assert_eq!(result[0].name, "get_weather");
        assert_eq!(result[1].id, Some("call_456".to_string()));
        assert_eq!(result[1].name, "search");
    }

    #[test]
    fn test_build_complete_tool_calls_missing_name() {
        let mut accumulators = HashMap::new();
        accumulators.insert(
            0,
            ToolCallAccumulator {
                id: Some("call_123".to_string()),
                name: None,
                arguments: r#"{}"#.to_string(),
            },
        );

        let result = build_complete_tool_calls(&accumulators);
        assert!(result.is_empty());
    }

    #[test]
    fn lines_without_data_carry_nothing() {
        assert!(parse(&["", ": keep-alive", "event: message", "data: {not json"]).is_empty());
    }

    #[test]
    fn a_complete_tool_call_in_one_delta_is_yielded_at_the_finish_reason() {
        let chunks = parse(&[
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"f","arguments":"{\"x\": 1}"}}]}}]}"#,
            r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
            "data: [DONE]",
        ]);

        let [StreamChunk::ToolCalls(calls)] = chunks.as_slice() else {
            panic!("expected one tool-calls chunk, got {chunks:?}");
        };
        assert_eq!(calls[0].name, "f");
        assert_eq!(calls[0].arguments["x"], 1);
    }
}
