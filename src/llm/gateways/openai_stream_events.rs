//! Completion rules for OpenAI-compatible server-sent event streams.

use super::openai::{openai_metadata, present};
use crate::llm::models::ResponseEvidence;
use crate::llm::stream_events::{invalid_event, FrameParser, StreamEvent, StreamEventError};
use serde_json::Value;

/// Reads `data:` lines from an OpenAI chat completion stream.
///
/// Success needs a `finish_reason` of `stop` and then `data: [DONE]`. Usage
/// arrives in a final chunk with no choices when `stream_options.include_usage`
/// is set.
#[derive(Default)]
pub(crate) struct OpenAiEventParser {
    evidence: ResponseEvidence,
}

impl FrameParser for OpenAiEventParser {
    fn parse_line(&mut self, line: &str) -> Vec<StreamEvent> {
        // Blank lines, comments and other SSE fields carry no event.
        let Some(data) = line.strip_prefix("data:") else {
            return Vec::new();
        };
        let data = data.trim_start();
        if data == "[DONE]" {
            return vec![self.terminal_event()];
        }

        let frame: Value = match serde_json::from_str(data) {
            Ok(frame) => frame,
            Err(error) => return vec![invalid_event(format!("frame is not JSON: {error}"))],
        };
        if let Some(error) = present(&frame["error"]) {
            return vec![StreamEvent::Error(StreamEventError::provider_frame(error))];
        }

        self.record_evidence(&frame);
        match frame["choices"].as_array().map(Vec::as_slice) {
            Some([]) => Vec::new(),
            Some([choice]) => self.parse_choice(choice),
            Some(_) => vec![invalid_event("frame has more than one choice")],
            None => vec![invalid_event("frame has no choices")],
        }
    }

    fn partial_evidence(&self) -> Option<ResponseEvidence> {
        (self.evidence != ResponseEvidence::default()).then(|| self.evidence.clone())
    }
}

impl OpenAiEventParser {
    fn record_evidence(&mut self, frame: &Value) {
        if let Some(model) = frame["model"].as_str() {
            self.evidence.provider_model = Some(model.to_string());
        }
        if let Some(usage) = present(&frame["usage"]) {
            self.evidence.usage = Some(usage);
        }
        self.evidence.metadata.extend(openai_metadata(frame));
    }

    fn parse_choice(&mut self, choice: &Value) -> Vec<StreamEvent> {
        if let Some(reason) = choice["finish_reason"].as_str() {
            self.evidence.finish_reason = Some(reason.to_string());
        }

        let delta = &choice["delta"];
        if !(delta.is_null() || delta.is_object()) {
            return vec![invalid_event("delta is not an object")];
        }
        if delta["tool_calls"].as_array().is_some_and(|calls| !calls.is_empty()) {
            return vec![StreamEvent::Error(StreamEventError::UnexpectedToolCalls)];
        }
        match &delta["content"] {
            Value::Null => Vec::new(),
            Value::String(text) if text.is_empty() => Vec::new(),
            Value::String(text) => vec![StreamEvent::Content(text.clone())],
            _ => vec![invalid_event("content is not a string")],
        }
    }

    fn terminal_event(&self) -> StreamEvent {
        let evidence = self.evidence.clone();
        if evidence.finish_reason.as_deref() == Some("stop") {
            StreamEvent::Completed(evidence)
        } else {
            StreamEvent::Error(StreamEventError::IncompleteCompletion(Box::new(evidence)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(lines: &[&str]) -> Vec<StreamEvent> {
        let mut parser = OpenAiEventParser::default();
        lines.iter().flat_map(|line| parser.parse_line(line)).collect()
    }

    const USAGE: &str = r#"{"prompt_tokens":5,"completion_tokens":2,"total_tokens":7}"#;

    #[test]
    fn content_then_stop_then_done_completes_with_evidence() {
        let usage_chunk =
            format!(r#"data: {{"id":"c1","model":"gpt-4o-2024","choices":[],"usage":{USAGE}}}"#);
        let events = parse(&[
            r#"data: {"id":"c1","model":"gpt-4o-2024","choices":[{"delta":{"role":"assistant","content":""}}]}"#,
            "",
            r#"data: {"id":"c1","model":"gpt-4o-2024","choices":[{"delta":{"content":"Hel"}}]}"#,
            r#"data: {"id":"c1","model":"gpt-4o-2024","choices":[{"delta":{"content":"lo"}}]}"#,
            r#"data: {"id":"c1","model":"gpt-4o-2024","choices":[{"delta":{},"finish_reason":"stop"}]}"#,
            &usage_chunk,
            "data: [DONE]",
        ]);

        assert!(matches!(&events[0], StreamEvent::Content(text) if text == "Hel"));
        assert!(matches!(&events[1], StreamEvent::Content(text) if text == "lo"));
        let [_, _, StreamEvent::Completed(evidence)] = events.as_slice() else {
            panic!("expected two content events then completed, got {events:?}");
        };
        assert_eq!(evidence.finish_reason.as_deref(), Some("stop"));
        assert_eq!(evidence.provider_model.as_deref(), Some("gpt-4o-2024"));
        assert_eq!(evidence.usage, Some(serde_json::from_str(USAGE).unwrap()));
        assert_eq!(evidence.metadata["id"], "c1");
    }

    #[test]
    fn done_after_length_is_incomplete_completion_with_evidence() {
        let usage_chunk = format!(r#"data: {{"choices":[],"usage":{USAGE}}}"#);
        let events = parse(&[
            r#"data: {"model":"gpt-4o","choices":[{"delta":{"content":"Partial"}}]}"#,
            r#"data: {"model":"gpt-4o","choices":[{"delta":{},"finish_reason":"length"}]}"#,
            &usage_chunk,
            "data: [DONE]",
        ]);

        let [StreamEvent::Content(_), StreamEvent::Error(StreamEventError::IncompleteCompletion(evidence))] =
            events.as_slice()
        else {
            panic!("expected content then incomplete completion, got {events:?}");
        };
        assert_eq!(evidence.finish_reason.as_deref(), Some("length"));
        assert_eq!(evidence.usage, Some(serde_json::from_str(USAGE).unwrap()));
        assert_eq!(evidence.provider_model.as_deref(), Some("gpt-4o"));
    }

    #[test]
    fn done_without_finish_reason_is_incomplete_completion() {
        let events = parse(&["data: [DONE]"]);

        assert!(matches!(
            events.as_slice(),
            [StreamEvent::Error(StreamEventError::IncompleteCompletion(evidence))]
                if evidence.finish_reason.is_none()
        ));
    }

    #[test]
    fn tool_call_delta_is_unexpected() {
        let events = parse(&[
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"f","arguments":""}}]}}]}"#,
        ]);

        assert!(matches!(
            events.as_slice(),
            [StreamEvent::Error(StreamEventError::UnexpectedToolCalls)]
        ));
    }

    #[test]
    fn error_frame_is_provider_error() {
        let events = parse(&[r#"data: {"error":{"message":"overloaded","type":"server_error"}}"#]);

        assert!(matches!(
            events.as_slice(),
            [StreamEvent::Error(StreamEventError::ProviderError { status: None, error })]
                if error["message"] == "overloaded"
        ));
    }

    #[test]
    fn malformed_frames_are_invalid_stream_events() {
        for line in [
            "data: {not json",
            r#"data: {"object":"chat.completion.chunk"}"#,
            r#"data: {"choices":[{"delta":{"content":42}}]}"#,
            r#"data: {"choices":[{"delta":"text"}]}"#,
            r#"data: {"choices":[{"delta":{}},{"delta":{}}]}"#,
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

    #[test]
    fn partial_evidence_holds_what_arrived_before_the_end() {
        let mut parser = OpenAiEventParser::default();
        assert_eq!(parser.partial_evidence(), None);

        parser.parse_line(r#"data: {"model":"gpt-4o-x","choices":[{"delta":{"content":"Hi"}}]}"#);

        let evidence = parser.partial_evidence().expect("model arrived");
        assert_eq!(evidence.provider_model.as_deref(), Some("gpt-4o-x"));
        assert_eq!(evidence.usage, None);
    }

    #[test]
    fn comments_and_other_fields_are_ignored() {
        assert!(parse(&[": keep-alive", "event: message", "id: 3", ""]).is_empty());
    }
}
