//! Tracer system for observability and debugging
//!
//! The tracer system provides comprehensive observability into LLM interactions,
//! tool executions, and agent communications. It records events with timestamps
//! and correlation IDs, enabling detailed debugging and monitoring.
//!
//! # Architecture
//!
//! The tracer system consists of several key components:
//!
//! - **TracerEvent**: Base trait for all event types with timestamps and correlation IDs
//! - **EventStore**: Thread-safe storage for events with callbacks and filtering
//! - **TracerSystem**: Coordination layer providing convenience methods for recording events
//! - **NullTracer**: Null object pattern for when tracing is disabled
//!
//! # Event Types
//!
//! - **LlmCallTracerEvent**: Records LLM calls with model, messages, temperature, and tools
//! - **LlmResponseTracerEvent**: Records LLM responses with content, tool calls, and duration
//! - **ToolCallTracerEvent**: Records tool executions with arguments, results, and duration
//! - **AgentInteractionTracerEvent**: Records agent-to-agent communications
//!
//! # Usage Example
//!
//! ```rust,ignore
//! use mojentic::tracer::{TracerSystem, LlmCallTracerEvent};
//! use std::collections::HashMap;
//!
//! // Create a tracer system
//! let tracer = TracerSystem::default();
//!
//! // Record an LLM call
//! tracer.record_llm_call(
//!     "llama3.2",
//!     vec![],
//!     0.7,
//!     None,
//!     "my_agent",
//!     "correlation-123"
//! );
//!
//! // Query events
//! let events = tracer.get_events(None, None, None);
//! for event in events {
//!     println!("{}", event.printable_summary());
//! }
//! ```
//!
//! # Correlation IDs
//!
//! Correlation IDs are UUIDs that are copied from cause-to-effect across the system,
//! enabling you to trace all events related to a single request or operation.
//! This creates a complete audit trail for debugging and observability.

pub mod event_store;
pub mod null_tracer;
pub mod tracer_events;
pub mod tracer_system;

// Re-export main types
pub use event_store::{EventCallback, EventStore};
pub use null_tracer::NullTracer;
pub use tracer_events::{
    AgentInteractionTracerEvent, EventFilterFn, LlmCallTracerEvent, LlmResponseTracerEvent,
    ToolCallTracerEvent, TracerEvent,
};
pub use tracer_system::TracerSystem;

#[cfg(test)]
pub(crate) mod testing {
    //! Test helpers that read typed events back out of a tracer.

    use super::*;
    use std::sync::{Arc, Mutex};

    /// A typed copy of an LLM event that a [`capturing_tracer`] recorded.
    #[derive(Debug, Clone)]
    pub(crate) enum CapturedLlmEvent {
        Call(LlmCallTracerEvent),
        Response(LlmResponseTracerEvent),
    }

    /// Captured LLM events, in recording order.
    pub(crate) type CapturedLlmEvents = Arc<Mutex<Vec<CapturedLlmEvent>>>;

    /// A tracer that also keeps a typed copy of every LLM call and response event.
    pub(crate) fn capturing_tracer() -> (Arc<TracerSystem>, CapturedLlmEvents) {
        let captured: CapturedLlmEvents = Arc::new(Mutex::new(Vec::new()));
        let sink = captured.clone();
        let store = EventStore::new(Some(Arc::new(move |event: &dyn TracerEvent| {
            let Some(any) = event.as_any() else { return };
            let copy = if let Some(call) = any.downcast_ref::<LlmCallTracerEvent>() {
                CapturedLlmEvent::Call(call.clone())
            } else if let Some(response) = any.downcast_ref::<LlmResponseTracerEvent>() {
                CapturedLlmEvent::Response(response.clone())
            } else {
                return;
            };
            sink.lock().unwrap().push(copy);
        })));
        (Arc::new(TracerSystem::new(Some(Arc::new(store)), true)), captured)
    }

    /// The LLM response events captured so far.
    pub(crate) fn responses(captured: &CapturedLlmEvents) -> Vec<LlmResponseTracerEvent> {
        captured
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                CapturedLlmEvent::Response(response) => Some(response.clone()),
                CapturedLlmEvent::Call(_) => None,
            })
            .collect()
    }
}
