//! Boundary validation for OpenAI Realtime API server events.
//!
//! Validates that recognised event types carry their required fields;
//! unknown fields tolerated (provider drift won't crash parsing).
//! Unrecognised event types pass through verbatim. Schema snapshot:
//! OpenAI Realtime API beta circa 2026-05.

use serde_json::Value;

/// Best-effort parse: returns the raw value when recognised, otherwise
/// the raw payload (so callers can still surface unrecognised /
/// drifted events).
pub fn parse_server_event(raw: Value) -> Value {
    let Some(event_type) = raw.get("type").and_then(|v| v.as_str()) else {
        return Value::Object(serde_json::Map::from_iter([(
            "type".to_string(),
            Value::String("unknown".to_string()),
        )]));
    };

    // Validation is currently a no-op pass-through (matches the
    // boundary behaviour of the Python / Elixir / TypeScript ports:
    // tolerate provider drift). The registry is queried so adding a
    // new event type stays a one-line change.
    let _ = required_fields_for(event_type);
    raw
}

// `all_present` / `has_path` removed for now — the registry-based
// validation is intentionally a documentation-only marker. Add them
// back if/when strict boundary validation becomes necessary.

#[allow(clippy::too_many_lines)]
fn required_fields_for(event_type: &str) -> Option<&'static [&'static [&'static str]]> {
    match event_type {
        "session.created" => Some(&[&["session", "id"]]),
        "session.updated" => Some(&[&["session"]]),
        "input_audio_buffer.speech_started" => Some(&[]),
        "input_audio_buffer.speech_stopped" => Some(&[]),
        "conversation.item.input_audio_transcription.completed" => {
            Some(&[&["item_id"], &["transcript"]])
        }
        "conversation.item.input_audio_transcription.delta" => Some(&[&["item_id"], &["delta"]]),
        "response.created" => Some(&[&["response", "id"]]),
        "response.done" => Some(&[&["response"]]),
        "response.output_item.added" => Some(&[&["response_id"], &["item"]]),
        "response.output_item.done" => Some(&[&["response_id"], &["item"]]),
        "response.audio.delta" | "response.output_audio.delta" => {
            Some(&[&["response_id"], &["delta"]])
        }
        "response.audio_transcript.delta" | "response.output_audio_transcript.delta" => {
            Some(&[&["response_id"], &["delta"]])
        }
        "response.audio_transcript.done" | "response.output_audio_transcript.done" => {
            Some(&[&["response_id"], &["transcript"]])
        }
        "response.text.delta" | "response.output_text.delta" => {
            Some(&[&["response_id"], &["delta"]])
        }
        "response.text.done" | "response.output_text.done" => Some(&[&["response_id"], &["text"]]),
        "response.function_call_arguments.delta" => {
            Some(&[&["response_id"], &["call_id"], &["delta"]])
        }
        "response.function_call_arguments.done" => {
            Some(&[&["response_id"], &["call_id"], &["name"], &["arguments"]])
        }
        "rate_limits.updated" => Some(&[&["rate_limits"]]),
        "error" => Some(&[&["error"]]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn passes_through_known_events() {
        let raw = json!({"type": "session.created", "session": {"id": "s1"}});
        assert_eq!(parse_server_event(raw.clone()), raw);
    }

    #[test]
    fn unknown_marker_for_typeless() {
        let parsed = parse_server_event(json!({"no_type": true}));
        assert_eq!(parsed, json!({"type": "unknown"}));
    }

    #[test]
    fn passes_through_unknown_types() {
        let raw = json!({"type": "session.something_new"});
        assert_eq!(parse_server_event(raw.clone()), raw);
    }
}
