//! Null tracer implementation following the Null Object Pattern
//!
//! This module provides a NullTracer that implements the same interface as TracerSystem
//! but performs no operations. This eliminates the need for conditional checks in client code.

use super::tracer_events::TracerEvent;
use std::collections::HashMap;

/// A no-op implementation of TracerSystem that silently discards all tracing operations
///
/// This class follows the Null Object Pattern to eliminate conditional checks in client code.
/// All record methods are overridden to do nothing, and all query methods return empty results.
pub struct NullTracer;

impl NullTracer {
    /// Create a new null tracer
    pub fn new() -> Self {
        Self
    }

    /// Always returns false for null tracer
    pub fn is_enabled(&self) -> bool {
        false
    }

    /// No-op method for interface compatibility
    pub fn enable(&self) {
        // Do nothing
    }

    /// No-op method for interface compatibility
    pub fn disable(&self) {
        // Do nothing
    }

    /// Do nothing implementation of record_event
    pub fn record_event(&self, _event: Box<dyn TracerEvent>) {
        // Do nothing
    }

    /// Do nothing implementation of record_llm_call
    #[allow(clippy::too_many_arguments)]
    pub fn record_llm_call(
        &self,
        _model: impl Into<String>,
        _messages: Vec<HashMap<String, serde_json::Value>>,
        _temperature: f64,
        _tools: Option<Vec<HashMap<String, serde_json::Value>>>,
        _source: impl Into<String>,
        _correlation_id: impl Into<String>,
    ) {
        // Do nothing
    }

    /// Do nothing implementation of record_llm_response
    pub fn record_llm_response(
        &self,
        _model: impl Into<String>,
        _content: impl Into<String>,
        _tool_calls: Option<Vec<HashMap<String, serde_json::Value>>>,
        _call_duration_ms: Option<f64>,
        _source: impl Into<String>,
        _correlation_id: impl Into<String>,
    ) {
        // Do nothing
    }

    /// Do nothing implementation of record_tool_call
    #[allow(clippy::too_many_arguments)]
    pub fn record_tool_call(
        &self,
        _tool_name: impl Into<String>,
        _arguments: HashMap<String, serde_json::Value>,
        _result: serde_json::Value,
        _caller: Option<String>,
        _call_duration_ms: Option<f64>,
        _source: impl Into<String>,
        _correlation_id: impl Into<String>,
    ) {
        // Do nothing
    }

    /// Do nothing implementation of record_agent_interaction
    pub fn record_agent_interaction(
        &self,
        _from_agent: impl Into<String>,
        _to_agent: impl Into<String>,
        _event_type: impl Into<String>,
        _event_id: Option<String>,
        _source: impl Into<String>,
        _correlation_id: impl Into<String>,
    ) {
        // Do nothing
    }

    /// Return an empty vector for any get_event_summaries request
    pub fn get_event_summaries(
        &self,
        _start_time: Option<f64>,
        _end_time: Option<f64>,
        _filter_func: Option<&dyn super::EventFilterFn>,
    ) -> Vec<String> {
        Vec::new()
    }

    /// Return an empty vector for any get_last_n_summaries request
    pub fn get_last_n_summaries(
        &self,
        _n: usize,
        _filter_func: Option<&dyn super::EventFilterFn>,
    ) -> Vec<String> {
        Vec::new()
    }

    /// Return 0 for any count_events request
    pub fn count_events(
        &self,
        _start_time: Option<f64>,
        _end_time: Option<f64>,
        _filter_func: Option<&dyn super::EventFilterFn>,
    ) -> usize {
        0
    }

    /// Do nothing implementation of clear method
    pub fn clear(&self) {
        // Do nothing
    }

    /// Always returns 0 for null tracer
    pub fn len(&self) -> usize {
        0
    }

    /// Always returns true for null tracer
    pub fn is_empty(&self) -> bool {
        true
    }
}

impl Default for NullTracer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null_tracer_is_disabled() {
        let tracer = NullTracer::new();
        assert!(!tracer.is_enabled());
    }

    #[test]
    fn test_null_tracer_enable_disable() {
        let tracer = NullTracer::new();
        tracer.enable();
        assert!(!tracer.is_enabled());

        tracer.disable();
        assert!(!tracer.is_enabled());
    }

    #[test]
    fn test_null_tracer_record_methods() {
        let tracer = NullTracer::new();

        // All record methods should be no-ops
        tracer.record_llm_call("llama3.2", vec![], 0.7, None, "test", "corr-123");

        tracer.record_llm_response(
            "llama3.2",
            "Hello, world!",
            None,
            Some(150.5),
            "test",
            "corr-456",
        );

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

        tracer.record_agent_interaction(
            "agent1",
            "agent2",
            "message",
            Some("evt-123".to_string()),
            "test",
            "corr-abc",
        );

        // Should have no events
        assert_eq!(tracer.len(), 0);
        assert!(tracer.is_empty());
    }

    #[test]
    fn test_null_tracer_query_methods() {
        let tracer = NullTracer::new();

        // All query methods should return empty results
        let summaries = tracer.get_event_summaries(None, None, None);
        assert!(summaries.is_empty());

        let last_summaries = tracer.get_last_n_summaries(10, None);
        assert!(last_summaries.is_empty());

        let count = tracer.count_events(None, None, None);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_null_tracer_clear() {
        let tracer = NullTracer::new();
        tracer.clear();
        assert!(tracer.is_empty());
    }

    #[test]
    fn test_null_tracer_len() {
        let tracer = NullTracer::new();
        assert_eq!(tracer.len(), 0);
    }
}
