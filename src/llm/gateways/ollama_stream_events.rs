//! Completion rules for Ollama's newline-delimited JSON chat stream.

use super::ollama::ollama_evidence;
use crate::llm::models::ResponseEvidence;
use crate::llm::stream_events::{invalid_event, FrameParser, StreamEvent, StreamEventError};
use serde_json::Value;

/// Reads frames from an Ollama `/api/chat` stream.
///
/// Success needs a final frame with `done: true` and a `done_reason` of
/// `stop`. Thinking text is not assistant content and is not yielded.
#[derive(Default)]
pub(crate) struct OllamaEventParser {
    /// Evidence from frames seen so far; the final frame replaces it.
    evidence: ResponseEvidence,
}

impl FrameParser for OllamaEventParser {
    fn parse_line(&mut self, line: &str) -> Vec<StreamEvent> {
        let line = line.trim();
        if line.is_empty() {
            return Vec::new();
        }

        let frame = match serde_json::from_str::<Value>(line) {
            Ok(frame) if frame.is_object() => frame,
            Ok(_) => return vec![invalid_event("frame is not a JSON object")],
            Err(error) => return vec![invalid_event(format!("frame is not JSON: {error}"))],
        };
        if !frame["error"].is_null() {
            return vec![StreamEvent::Error(StreamEventError::provider_frame(
                frame["error"].clone(),
            ))];
        }
        self.evidence = ollama_evidence(&frame);

        let message = &frame["message"];
        if message["tool_calls"].as_array().is_some_and(|calls| !calls.is_empty()) {
            return vec![StreamEvent::Error(StreamEventError::UnexpectedToolCalls)];
        }

        let mut events = match &message["content"] {
            Value::Null => Vec::new(),
            Value::String(text) if text.is_empty() => Vec::new(),
            Value::String(text) => vec![StreamEvent::Content(text.clone())],
            _ => return vec![invalid_event("content is not a string")],
        };
        if frame["done"].as_bool() == Some(true) {
            events.push(terminal_event(self.evidence.clone()));
        }
        events
    }

    fn partial_evidence(&self) -> Option<ResponseEvidence> {
        (self.evidence != ResponseEvidence::default()).then(|| self.evidence.clone())
    }
}

fn terminal_event(evidence: ResponseEvidence) -> StreamEvent {
    if evidence.finish_reason.as_deref() == Some("stop") {
        StreamEvent::Completed(evidence)
    } else {
        StreamEvent::Error(StreamEventError::IncompleteCompletion(Box::new(evidence)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(lines: &[&str]) -> Vec<StreamEvent> {
        let mut parser = OllamaEventParser::default();
        lines.iter().flat_map(|line| parser.parse_line(line)).collect()
    }

    const FINAL_STOP: &str = r#"{"model":"qwen3:32b","created_at":"2026-09-24T10:00:00Z","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","total_duration":900,"load_duration":100,"prompt_eval_count":12,"prompt_eval_duration":200,"eval_count":34,"eval_duration":600}"#;

    #[test]
    fn content_then_done_with_stop_completes_with_evidence() {
        let events = parse(&[
            r#"{"model":"qwen3:32b","message":{"role":"assistant","thinking":"hmm","content":""},"done":false}"#,
            r#"{"model":"qwen3:32b","message":{"role":"assistant","content":"Hel"},"done":false}"#,
            r#"{"model":"qwen3:32b","message":{"role":"assistant","content":"lo"},"done":false}"#,
            FINAL_STOP,
        ]);

        let [StreamEvent::Content(first), StreamEvent::Content(second), StreamEvent::Completed(evidence)] =
            events.as_slice()
        else {
            panic!("expected two content events then completed, got {events:?}");
        };
        assert_eq!((first.as_str(), second.as_str()), ("Hel", "lo"));
        assert_eq!(evidence.finish_reason.as_deref(), Some("stop"));
        assert_eq!(evidence.provider_model.as_deref(), Some("qwen3:32b"));
        assert_eq!(
            evidence.usage,
            Some(serde_json::json!({"prompt_eval_count": 12, "eval_count": 34}))
        );
        assert_eq!(evidence.metadata["total_duration"], 900);
        assert_eq!(evidence.metadata["load_duration"], 100);
        assert_eq!(evidence.metadata["prompt_eval_duration"], 200);
        assert_eq!(evidence.metadata["eval_duration"], 600);
    }

    #[test]
    fn content_in_the_final_frame_precedes_the_terminal_event() {
        let events = parse(&[r#"{"message":{"content":"tail"},"done":true,"done_reason":"stop"}"#]);

        assert!(matches!(
            events.as_slice(),
            [StreamEvent::Content(text), StreamEvent::Completed(_)] if text == "tail"
        ));
    }

    #[test]
    fn done_with_length_is_incomplete_completion_with_evidence() {
        let events = parse(&[
            r#"{"message":{"content":"Partial"},"done":false}"#,
            r#"{"model":"qwen3:32b","done":true,"done_reason":"length","prompt_eval_count":3,"eval_count":4}"#,
        ]);

        let [StreamEvent::Content(_), StreamEvent::Error(StreamEventError::IncompleteCompletion(evidence))] =
            events.as_slice()
        else {
            panic!("expected content then incomplete completion, got {events:?}");
        };
        assert_eq!(evidence.finish_reason.as_deref(), Some("length"));
        assert_eq!(
            evidence.usage,
            Some(serde_json::json!({"prompt_eval_count": 3, "eval_count": 4}))
        );
    }

    #[test]
    fn done_without_reason_is_incomplete_completion() {
        let events = parse(&[r#"{"done":true}"#]);

        assert!(matches!(
            events.as_slice(),
            [StreamEvent::Error(StreamEventError::IncompleteCompletion(evidence))]
                if evidence.finish_reason.is_none()
        ));
    }

    #[test]
    fn tool_call_frame_is_unexpected() {
        let events = parse(&[
            r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"f","arguments":{}}}]},"done":false}"#,
        ]);

        assert!(matches!(
            events.as_slice(),
            [StreamEvent::Error(StreamEventError::UnexpectedToolCalls)]
        ));
    }

    #[test]
    fn error_frame_is_provider_error() {
        let events = parse(&[r#"{"error":"model 'nope' not found"}"#]);

        assert!(matches!(
            events.as_slice(),
            [StreamEvent::Error(StreamEventError::ProviderError { status: None, error })]
                if error == "model 'nope' not found"
        ));
    }

    #[test]
    fn partial_evidence_holds_the_model_seen_before_the_end() {
        let mut parser = OllamaEventParser::default();
        assert_eq!(parser.partial_evidence(), None);

        parser.parse_line(r#"{"model":"qwen3:32b","message":{"content":"Hi"},"done":false}"#);

        let evidence = parser.partial_evidence().expect("model arrived");
        assert_eq!(evidence.provider_model.as_deref(), Some("qwen3:32b"));
    }

    #[test]
    fn malformed_frames_are_invalid_stream_events() {
        for line in [
            "{not json",
            "[1,2]",
            r#"{"message":{"content":7},"done":false}"#,
        ] {
            let events = parse(&[line]);
            assert!(
                matches!(
                    events.as_slice(),
                    [StreamEvent::Error(StreamEventError::InvalidStreamEvent(_))]
                ),
                "{line} gave {events:?}"
            );
        }
    }
}
