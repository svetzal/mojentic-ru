//! Boundary handling for OpenAI Realtime API server events.
//!
//! Tolerates provider drift: unknown event types and unknown fields pass
//! through verbatim. Typeless payloads are tagged with `type: "unknown"`
//! so the session driver can ignore them deterministically. Matches the
//! Python / Elixir / TypeScript boundary behaviour.

use serde_json::Value;

/// Best-effort parse: returns the raw value as-is when it has a `type`
/// field, otherwise tags it `unknown` so the session driver's match
/// arm sees a stable shape.
pub fn parse_server_event(raw: Value) -> Value {
    if raw.get("type").and_then(|v| v.as_str()).is_some() {
        raw
    } else {
        Value::Object(serde_json::Map::from_iter([(
            "type".to_string(),
            Value::String("unknown".to_string()),
        )]))
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
