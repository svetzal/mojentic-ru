//! Tracer event types for tracking system interactions
//!
//! This module defines the core event types used by the tracer system to record
//! LLM calls, tool executions, and agent interactions. All events implement the
//! `TracerEvent` trait which provides timestamps, correlation IDs, and printable summaries.

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Trait for filtering tracer events
///
/// Implement this trait to create custom event filters. This trait is used
/// instead of raw closure types to avoid type complexity warnings.
pub trait EventFilterFn: Send + Sync {
    /// Test whether an event passes the filter
    fn matches(&self, event: &dyn TracerEvent) -> bool;
}

/// Implement EventFilterFn for any function that matches the signature
impl<F> EventFilterFn for F
where
    F: Fn(&dyn TracerEvent) -> bool + Send + Sync,
{
    fn matches(&self, event: &dyn TracerEvent) -> bool {
        self(event)
    }
}

/// Base trait for all tracer events
///
/// Tracer events are used to track system interactions for observability purposes.
/// They are distinct from regular events which are used for agent communication.
pub trait TracerEvent: Send + Sync {
    /// Get the timestamp when the event occurred
    fn timestamp(&self) -> f64;

    /// Get the correlation ID for tracing related events
    fn correlation_id(&self) -> &str;

    /// Get the source of the event
    fn source(&self) -> &str;

    /// Get a formatted string summary of the event
    fn printable_summary(&self) -> String;
}

/// Records when an LLM is called with specific messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCallTracerEvent {
    /// Timestamp when the event occurred (Unix timestamp)
    pub timestamp: f64,
    /// UUID string that is copied from cause-to-affect for tracing events
    pub correlation_id: String,
    /// Source of the event
    pub source: String,
    /// The LLM model that was used
    pub model: String,
    /// The messages sent to the LLM (simplified representation)
    pub messages: Vec<HashMap<String, serde_json::Value>>,
    /// The temperature setting used for the call
    pub temperature: f64,
    /// The tools available to the LLM, if any
    pub tools: Option<Vec<HashMap<String, serde_json::Value>>>,
}

impl TracerEvent for LlmCallTracerEvent {
    fn timestamp(&self) -> f64 {
        self.timestamp
    }

    fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    fn source(&self) -> &str {
        &self.source
    }

    fn printable_summary(&self) -> String {
        let dt = DateTime::from_timestamp(self.timestamp as i64, 0)
            .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap())
            .with_timezone(&Local);
        let time_str = dt.format("%H:%M:%S%.3f").to_string();

        let mut summary = format!(
            "[{}] LlmCallTracerEvent (correlation_id: {})\n   Model: {}",
            time_str, self.correlation_id, self.model
        );

        if !self.messages.is_empty() {
            let msg_count = self.messages.len();
            let plural = if msg_count != 1 { "s" } else { "" };
            summary.push_str(&format!("\n   Messages: {} message{}", msg_count, plural));
        }

        if (self.temperature - 1.0).abs() > f64::EPSILON {
            summary.push_str(&format!("\n   Temperature: {}", self.temperature));
        }

        if let Some(tools) = &self.tools {
            let tool_names: Vec<String> = tools
                .iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                .collect();
            if !tool_names.is_empty() {
                summary.push_str(&format!("\n   Available Tools: {}", tool_names.join(", ")));
            }
        }

        summary
    }
}

/// Records when an LLM responds to a call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponseTracerEvent {
    /// Timestamp when the event occurred (Unix timestamp)
    pub timestamp: f64,
    /// UUID string that is copied from cause-to-affect for tracing events
    pub correlation_id: String,
    /// Source of the event
    pub source: String,
    /// The LLM model that was used
    pub model: String,
    /// The content of the LLM response
    pub content: String,
    /// Any tool calls made by the LLM
    pub tool_calls: Option<Vec<HashMap<String, serde_json::Value>>>,
    /// Duration of the LLM call in milliseconds
    pub call_duration_ms: Option<f64>,
}

impl TracerEvent for LlmResponseTracerEvent {
    fn timestamp(&self) -> f64 {
        self.timestamp
    }

    fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    fn source(&self) -> &str {
        &self.source
    }

    fn printable_summary(&self) -> String {
        let dt = DateTime::from_timestamp(self.timestamp as i64, 0)
            .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap())
            .with_timezone(&Local);
        let time_str = dt.format("%H:%M:%S%.3f").to_string();

        let mut summary = format!(
            "[{}] LlmResponseTracerEvent (correlation_id: {})\n   Model: {}",
            time_str, self.correlation_id, self.model
        );

        if !self.content.is_empty() {
            let content_preview = if self.content.len() > 100 {
                format!("{}...", &self.content[..100])
            } else {
                self.content.clone()
            };
            summary.push_str(&format!("\n   Content: {}", content_preview));
        }

        if let Some(tool_calls) = &self.tool_calls {
            let tool_count = tool_calls.len();
            let plural = if tool_count != 1 { "s" } else { "" };
            summary.push_str(&format!("\n   Tool Calls: {} call{}", tool_count, plural));
        }

        if let Some(duration) = self.call_duration_ms {
            summary.push_str(&format!("\n   Duration: {:.2}ms", duration));
        }

        summary
    }
}

/// Records when a tool is called during agent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallTracerEvent {
    /// Timestamp when the event occurred (Unix timestamp)
    pub timestamp: f64,
    /// UUID string that is copied from cause-to-affect for tracing events
    pub correlation_id: String,
    /// Source of the event
    pub source: String,
    /// Name of the tool that was called
    pub tool_name: String,
    /// Arguments provided to the tool
    pub arguments: HashMap<String, serde_json::Value>,
    /// Result returned by the tool (as JSON value)
    pub result: serde_json::Value,
    /// Name of the agent or component that called the tool
    pub caller: Option<String>,
    /// Duration of the tool call in milliseconds
    pub call_duration_ms: Option<f64>,
}

impl TracerEvent for ToolCallTracerEvent {
    fn timestamp(&self) -> f64 {
        self.timestamp
    }

    fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    fn source(&self) -> &str {
        &self.source
    }

    fn printable_summary(&self) -> String {
        let dt = DateTime::from_timestamp(self.timestamp as i64, 0)
            .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap())
            .with_timezone(&Local);
        let time_str = dt.format("%H:%M:%S%.3f").to_string();

        let mut summary = format!(
            "[{}] ToolCallTracerEvent (correlation_id: {})\n   Tool: {}",
            time_str, self.correlation_id, self.tool_name
        );

        if !self.arguments.is_empty() {
            summary.push_str(&format!("\n   Arguments: {:?}", self.arguments));
        }

        let result_str = self.result.to_string();
        let result_preview = if result_str.len() > 100 {
            format!("{}...", &result_str[..100])
        } else {
            result_str
        };
        summary.push_str(&format!("\n   Result: {}", result_preview));

        if let Some(caller) = &self.caller {
            summary.push_str(&format!("\n   Caller: {}", caller));
        }

        if let Some(duration) = self.call_duration_ms {
            summary.push_str(&format!("\n   Duration: {:.2}ms", duration));
        }

        summary
    }
}

/// Records interactions between agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInteractionTracerEvent {
    /// Timestamp when the event occurred (Unix timestamp)
    pub timestamp: f64,
    /// UUID string that is copied from cause-to-affect for tracing events
    pub correlation_id: String,
    /// Source of the event
    pub source: String,
    /// Name of the agent sending the event
    pub from_agent: String,
    /// Name of the agent receiving the event
    pub to_agent: String,
    /// Type of event being processed
    pub event_type: String,
    /// Unique identifier for the event
    pub event_id: Option<String>,
}

impl TracerEvent for AgentInteractionTracerEvent {
    fn timestamp(&self) -> f64 {
        self.timestamp
    }

    fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    fn source(&self) -> &str {
        &self.source
    }

    fn printable_summary(&self) -> String {
        let dt = DateTime::from_timestamp(self.timestamp as i64, 0)
            .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap())
            .with_timezone(&Local);
        let time_str = dt.format("%H:%M:%S%.3f").to_string();

        let mut summary = format!(
            "[{}] AgentInteractionTracerEvent (correlation_id: {})\n   From: {} → To: {}\n   Event Type: {}",
            time_str, self.correlation_id, self.from_agent, self.to_agent, self.event_type
        );

        if let Some(event_id) = &self.event_id {
            summary.push_str(&format!("\n   Event ID: {}", event_id));
        }

        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn current_timestamp() -> f64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
    }

    #[test]
    fn test_llm_call_event() {
        let event = LlmCallTracerEvent {
            timestamp: current_timestamp(),
            correlation_id: "test-123".to_string(),
            source: "test".to_string(),
            model: "llama3.2".to_string(),
            messages: vec![],
            temperature: 0.7,
            tools: None,
        };

        assert_eq!(event.correlation_id(), "test-123");
        assert_eq!(event.model, "llama3.2");
        assert!((event.temperature - 0.7).abs() < f64::EPSILON);

        let summary = event.printable_summary();
        assert!(summary.contains("LlmCallTracerEvent"));
        assert!(summary.contains("test-123"));
        assert!(summary.contains("llama3.2"));
    }

    #[test]
    fn test_llm_response_event() {
        let event = LlmResponseTracerEvent {
            timestamp: current_timestamp(),
            correlation_id: "test-456".to_string(),
            source: "test".to_string(),
            model: "llama3.2".to_string(),
            content: "Hello, world!".to_string(),
            tool_calls: None,
            call_duration_ms: Some(150.5),
        };

        assert_eq!(event.content, "Hello, world!");
        assert_eq!(event.call_duration_ms, Some(150.5));

        let summary = event.printable_summary();
        assert!(summary.contains("LlmResponseTracerEvent"));
        assert!(summary.contains("Hello, world!"));
        assert!(summary.contains("150.5"));
    }

    #[test]
    fn test_tool_call_event() {
        let mut args = HashMap::new();
        args.insert("input".to_string(), serde_json::json!("test"));

        let event = ToolCallTracerEvent {
            timestamp: current_timestamp(),
            correlation_id: "test-789".to_string(),
            source: "test".to_string(),
            tool_name: "example_tool".to_string(),
            arguments: args,
            result: serde_json::json!({"output": "result"}),
            caller: Some("agent1".to_string()),
            call_duration_ms: Some(25.0),
        };

        assert_eq!(event.tool_name, "example_tool");
        assert_eq!(event.caller, Some("agent1".to_string()));

        let summary = event.printable_summary();
        assert!(summary.contains("ToolCallTracerEvent"));
        assert!(summary.contains("example_tool"));
    }

    #[test]
    fn test_agent_interaction_event() {
        let event = AgentInteractionTracerEvent {
            timestamp: current_timestamp(),
            correlation_id: "test-abc".to_string(),
            source: "test".to_string(),
            from_agent: "agent1".to_string(),
            to_agent: "agent2".to_string(),
            event_type: "message".to_string(),
            event_id: Some("evt-123".to_string()),
        };

        assert_eq!(event.from_agent, "agent1");
        assert_eq!(event.to_agent, "agent2");

        let summary = event.printable_summary();
        assert!(summary.contains("AgentInteractionTracerEvent"));
        assert!(summary.contains("agent1"));
        assert!(summary.contains("agent2"));
    }
}
