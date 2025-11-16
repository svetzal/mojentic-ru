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
    AgentInteractionTracerEvent, LlmCallTracerEvent, LlmResponseTracerEvent, ToolCallTracerEvent,
    TracerEvent,
};
pub use tracer_system::TracerSystem;
