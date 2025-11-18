//! Tracer system for coordinating tracer events
//!
//! This module provides the central system for recording, filtering, and querying
//! tracer events. It coordinates with the event store and provides convenience methods
//! for recording different types of events.

use super::event_store::EventStore;
use super::tracer_events::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Central system for capturing and querying tracer events
///
/// The TracerSystem is responsible for recording events related to LLM calls,
/// tool usage, and agent interactions, providing a way to trace through the
/// major events of the system.
pub struct TracerSystem {
    event_store: Arc<EventStore>,
    enabled: Arc<AtomicBool>,
}

impl TracerSystem {
    /// Create a new tracer system
    ///
    /// # Arguments
    ///
    /// * `event_store` - Optional event store to use. If None, a new one will be created.
    /// * `enabled` - Whether the tracer system is enabled (default: true)
    pub fn new(event_store: Option<Arc<EventStore>>, enabled: bool) -> Self {
        Self {
            event_store: event_store.unwrap_or_else(|| Arc::new(EventStore::default())),
            enabled: Arc::new(AtomicBool::new(enabled)),
        }
    }

    /// Check if the tracer is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Enable the tracer system
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::SeqCst);
    }

    /// Disable the tracer system
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::SeqCst);
    }

    /// Record a tracer event in the event store
    ///
    /// # Arguments
    ///
    /// * `event` - The tracer event to record
    pub fn record_event(&self, event: Box<dyn TracerEvent>) {
        if !self.is_enabled() {
            return;
        }
        self.event_store.store(event);
    }

    /// Record an LLM call event
    ///
    /// # Arguments
    ///
    /// * `model` - The name of the LLM model being called
    /// * `messages` - The messages sent to the LLM (simplified representation)
    /// * `temperature` - The temperature setting for the LLM call
    /// * `tools` - The tools available to the LLM, if any
    /// * `source` - The source of the event
    /// * `correlation_id` - UUID string for tracing related events
    pub fn record_llm_call(
        &self,
        model: impl Into<String>,
        messages: Vec<HashMap<String, serde_json::Value>>,
        temperature: f64,
        tools: Option<Vec<HashMap<String, serde_json::Value>>>,
        source: impl Into<String>,
        correlation_id: impl Into<String>,
    ) {
        if !self.is_enabled() {
            return;
        }

        let event = Box::new(LlmCallTracerEvent {
            timestamp: current_timestamp(),
            correlation_id: correlation_id.into(),
            source: source.into(),
            model: model.into(),
            messages,
            temperature,
            tools,
        });

        self.event_store.store(event);
    }

    /// Record an LLM response event
    ///
    /// # Arguments
    ///
    /// * `model` - The name of the LLM model that responded
    /// * `content` - The content of the LLM response
    /// * `tool_calls` - Any tool calls made by the LLM in its response
    /// * `call_duration_ms` - The duration of the LLM call in milliseconds
    /// * `source` - The source of the event
    /// * `correlation_id` - UUID string for tracing related events
    pub fn record_llm_response(
        &self,
        model: impl Into<String>,
        content: impl Into<String>,
        tool_calls: Option<Vec<HashMap<String, serde_json::Value>>>,
        call_duration_ms: Option<f64>,
        source: impl Into<String>,
        correlation_id: impl Into<String>,
    ) {
        if !self.is_enabled() {
            return;
        }

        let event = Box::new(LlmResponseTracerEvent {
            timestamp: current_timestamp(),
            correlation_id: correlation_id.into(),
            source: source.into(),
            model: model.into(),
            content: content.into(),
            tool_calls,
            call_duration_ms,
        });

        self.event_store.store(event);
    }

    /// Record a tool call event
    ///
    /// # Arguments
    ///
    /// * `tool_name` - The name of the tool being called
    /// * `arguments` - The arguments provided to the tool
    /// * `result` - The result returned by the tool
    /// * `caller` - The name of the agent or component calling the tool
    /// * `call_duration_ms` - The duration of the tool call in milliseconds
    /// * `source` - The source of the event
    /// * `correlation_id` - UUID string for tracing related events
    #[allow(clippy::too_many_arguments)]
    pub fn record_tool_call(
        &self,
        tool_name: impl Into<String>,
        arguments: HashMap<String, serde_json::Value>,
        result: serde_json::Value,
        caller: Option<String>,
        call_duration_ms: Option<f64>,
        source: impl Into<String>,
        correlation_id: impl Into<String>,
    ) {
        if !self.is_enabled() {
            return;
        }

        let event = Box::new(ToolCallTracerEvent {
            timestamp: current_timestamp(),
            correlation_id: correlation_id.into(),
            source: source.into(),
            tool_name: tool_name.into(),
            arguments,
            result,
            caller,
            call_duration_ms,
        });

        self.event_store.store(event);
    }

    /// Record an agent interaction event
    ///
    /// # Arguments
    ///
    /// * `from_agent` - The name of the agent sending the event
    /// * `to_agent` - The name of the agent receiving the event
    /// * `event_type` - The type of event being processed
    /// * `event_id` - A unique identifier for the event
    /// * `source` - The source of the event
    /// * `correlation_id` - UUID string for tracing related events
    pub fn record_agent_interaction(
        &self,
        from_agent: impl Into<String>,
        to_agent: impl Into<String>,
        event_type: impl Into<String>,
        event_id: Option<String>,
        source: impl Into<String>,
        correlation_id: impl Into<String>,
    ) {
        if !self.is_enabled() {
            return;
        }

        let event = Box::new(AgentInteractionTracerEvent {
            timestamp: current_timestamp(),
            correlation_id: correlation_id.into(),
            source: source.into(),
            from_agent: from_agent.into(),
            to_agent: to_agent.into(),
            event_type: event_type.into(),
            event_id,
        });

        self.event_store.store(event);
    }

    /// Get event summaries from the store, optionally filtered
    ///
    /// # Arguments
    ///
    /// * `start_time` - Include events with timestamp >= start_time
    /// * `end_time` - Include events with timestamp <= end_time
    /// * `filter_func` - Custom filter function to apply to events
    ///
    /// # Returns
    ///
    /// Vector of event summaries matching the filter criteria
    pub fn get_event_summaries(
        &self,
        start_time: Option<f64>,
        end_time: Option<f64>,
        filter_func: Option<&dyn super::EventFilterFn>,
    ) -> Vec<String> {
        self.event_store.get_event_summaries(start_time, end_time, filter_func)
    }

    /// Get the last N event summaries, optionally filtered
    ///
    /// # Arguments
    ///
    /// * `n` - Number of events to return
    /// * `filter_func` - Optional custom filter function
    ///
    /// # Returns
    ///
    /// Vector of the last N event summaries matching the filter criteria
    pub fn get_last_n_summaries(
        &self,
        n: usize,
        filter_func: Option<&dyn super::EventFilterFn>,
    ) -> Vec<String> {
        self.event_store.get_last_n_summaries(n, filter_func)
    }

    /// Count events matching filters
    ///
    /// # Arguments
    ///
    /// * `start_time` - Include events with timestamp >= start_time
    /// * `end_time` - Include events with timestamp <= end_time
    /// * `filter_func` - Custom filter function to apply to events
    ///
    /// # Returns
    ///
    /// Number of events matching the filter criteria
    pub fn count_events(
        &self,
        start_time: Option<f64>,
        end_time: Option<f64>,
        filter_func: Option<&dyn super::EventFilterFn>,
    ) -> usize {
        self.event_store.count_events(start_time, end_time, filter_func)
    }

    /// Clear all events from the event store
    pub fn clear(&self) {
        self.event_store.clear();
    }

    /// Get the total number of events in the store
    pub fn len(&self) -> usize {
        self.event_store.len()
    }

    /// Check if the event store is empty
    pub fn is_empty(&self) -> bool {
        self.event_store.is_empty()
    }
}

impl Default for TracerSystem {
    fn default() -> Self {
        Self::new(None, true)
    }
}

/// Get current timestamp as Unix timestamp (seconds since epoch)
fn current_timestamp() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_tracer_system() {
        let tracer = TracerSystem::default();
        assert!(tracer.is_enabled());
        assert_eq!(tracer.len(), 0);
    }

    #[test]
    fn test_enable_disable() {
        let tracer = TracerSystem::default();
        assert!(tracer.is_enabled());

        tracer.disable();
        assert!(!tracer.is_enabled());

        tracer.enable();
        assert!(tracer.is_enabled());
    }

    #[test]
    fn test_record_llm_call() {
        let tracer = TracerSystem::default();

        tracer.record_llm_call("llama3.2", vec![], 0.7, None, "test", "corr-123");

        assert_eq!(tracer.len(), 1);
    }

    #[test]
    fn test_record_llm_response() {
        let tracer = TracerSystem::default();

        tracer.record_llm_response(
            "llama3.2",
            "Hello, world!",
            None,
            Some(150.5),
            "test",
            "corr-456",
        );

        assert_eq!(tracer.len(), 1);
    }

    #[test]
    fn test_record_tool_call() {
        let tracer = TracerSystem::default();
        let mut args = HashMap::new();
        args.insert("input".to_string(), serde_json::json!("test"));

        tracer.record_tool_call(
            "example_tool",
            args,
            serde_json::json!({"output": "result"}),
            Some("agent1".to_string()),
            Some(25.0),
            "test",
            "corr-789",
        );

        assert_eq!(tracer.len(), 1);
    }

    #[test]
    fn test_record_agent_interaction() {
        let tracer = TracerSystem::default();

        tracer.record_agent_interaction(
            "agent1",
            "agent2",
            "message",
            Some("evt-123".to_string()),
            "test",
            "corr-abc",
        );

        assert_eq!(tracer.len(), 1);
    }

    #[test]
    fn test_disabled_tracer_doesnt_record() {
        let tracer = TracerSystem::new(None, false);
        assert!(!tracer.is_enabled());

        tracer.record_llm_call("llama3.2", vec![], 1.0, None, "test", "corr-123");

        assert_eq!(tracer.len(), 0);
    }

    #[test]
    fn test_clear() {
        let tracer = TracerSystem::default();

        tracer.record_llm_call("llama3.2", vec![], 1.0, None, "test", "corr-123");

        assert_eq!(tracer.len(), 1);

        tracer.clear();
        assert_eq!(tracer.len(), 0);
        assert!(tracer.is_empty());
    }

    #[test]
    fn test_multiple_events() {
        let tracer = TracerSystem::default();

        for i in 0..5 {
            tracer.record_llm_call("llama3.2", vec![], 1.0, None, "test", format!("corr-{}", i));
        }

        assert_eq!(tracer.len(), 5);
    }
}
