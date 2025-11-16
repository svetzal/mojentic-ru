# Tracer System

The tracer system provides comprehensive observability into LLM interactions, tool executions, and agent communications. It records events with timestamps and correlation IDs, enabling detailed debugging and monitoring.

Goals:
- Performance insight
- Failure localization
- Reproducibility of agent flows

## Architecture

The tracer system consists of several key components:

### Core Components

- **TracerEvent**: Base trait for all event types with timestamps, correlation IDs, and printable summaries
- **EventStore**: Thread-safe storage for events with callbacks and filtering capabilities
- **TracerSystem**: Coordination layer providing convenience methods for recording events
- **NullTracer**: Null object pattern implementation for when tracing is disabled

### Event Types

The system supports four main event types:

1. **LlmCallTracerEvent**: Records LLM calls with model, messages, temperature, and available tools
2. **LlmResponseTracerEvent**: Records LLM responses with content, tool calls, and duration
3. **ToolCallTracerEvent**: Records tool executions with arguments, results, and duration
4. **AgentInteractionTracerEvent**: Records agent-to-agent communications

## Usage

### Basic Setup

```rust
use mojentic::tracer::TracerSystem;
use std::sync::Arc;

// Create a tracer system
let tracer = Arc::new(TracerSystem::default());

// Enable/disable tracing
tracer.enable();
tracer.disable();
```

### Recording Events

#### LLM Calls

```rust
use std::collections::HashMap;

tracer.record_llm_call(
    "llama3.2",                    // model
    vec![],                        // messages (simplified)
    0.7,                           // temperature
    None,                          // tools
    "my_broker",                   // source
    "correlation-123"              // correlation_id
);
```

#### LLM Responses

```rust
tracer.record_llm_response(
    "llama3.2",                    // model
    "Response text",               // content
    None,                          // tool_calls
    Some(150.5),                   // call_duration_ms
    "my_broker",                   // source
    "correlation-123"              // correlation_id
);
```

#### Tool Calls

```rust
use serde_json::json;

let mut arguments = HashMap::new();
arguments.insert("input".to_string(), json!("test data"));

tracer.record_tool_call(
    "date_tool",                   // tool_name
    arguments,                     // arguments
    json!({"result": "2024-01-15"}), // result
    Some("agent1".to_string()),    // caller
    Some(25.0),                    // call_duration_ms
    "tool_executor",               // source
    "correlation-123"              // correlation_id
);
```

#### Agent Interactions

```rust
tracer.record_agent_interaction(
    "agent1",                      // from_agent
    "agent2",                      // to_agent
    "request",                     // event_type
    Some("evt-456".to_string()),   // event_id
    "dispatcher",                  // source
    "correlation-123"              // correlation_id
);
```

### Querying Events

#### Get All Events

```rust
let summaries = tracer.get_event_summaries(None, None, None);
for summary in summaries {
    println!("{}", summary);
}
```

#### Filter by Time Range

```rust
use std::time::{SystemTime, UNIX_EPOCH};

let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_secs_f64();

let one_hour_ago = now - 3600.0;

let recent_events = tracer.get_event_summaries(
    Some(one_hour_ago),  // start_time
    Some(now),           // end_time
    None                 // filter_func
);
```

#### Filter by Custom Predicate

```rust
// Find all events from a specific correlation ID
let correlation_id = "correlation-123";
let related_events = tracer.get_event_summaries(
    None,
    None,
    Some(&|event| event.correlation_id() == correlation_id)
);

// Count tool call events
let tool_call_count = tracer.count_events(
    None,
    None,
    Some(&|event| {
        event.printable_summary().contains("ToolCallTracerEvent")
    })
);
```

#### Get Last N Events

```rust
// Get last 10 events
let last_events = tracer.get_last_n_summaries(10, None);

// Get last 5 LLM-related events
let last_llm_events = tracer.get_last_n_summaries(
    5,
    Some(&|event| {
        let summary = event.printable_summary();
        summary.contains("LlmCallTracerEvent") ||
        summary.contains("LlmResponseTracerEvent")
    })
);
```

### Managing Events

```rust
// Get event count
let total_events = tracer.len();
println!("Total events: {}", total_events);

// Check if empty
if tracer.is_empty() {
    println!("No events recorded");
}

// Clear all events
tracer.clear();
```

## Correlation IDs

Correlation IDs are UUIDs that track related events across the system. They enable you to trace all events related to a single request or operation, creating a complete audit trail.

### Best Practices

1. **Generate Once**: Create a correlation ID at the start of a request
2. **Pass Through**: Copy the correlation ID to all downstream operations
3. **Query Together**: Use correlation IDs to filter related events

Example flow:
```
User Request → Generate correlation_id
  ↓
LLM Call (correlation_id: "abc-123")
  ↓
LLM Response (correlation_id: "abc-123")
  ↓
Tool Call (correlation_id: "abc-123")
  ↓
LLM Call with tool result (correlation_id: "abc-123")
  ↓
Final LLM Response (correlation_id: "abc-123")
```

## Event Store Callbacks

You can register a callback function that's called whenever an event is stored:

```rust
use mojentic::tracer::{EventStore, EventCallback, TracerSystem};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

// Create a counter
let event_count = Arc::new(AtomicUsize::new(0));
let count_clone = Arc::clone(&event_count);

// Create callback
let callback: EventCallback = Arc::new(move |event| {
    count_clone.fetch_add(1, Ordering::SeqCst);
    println!("Event stored: {}", event.printable_summary());
});

// Create event store with callback
let event_store = Arc::new(EventStore::new(Some(callback)));

// Create tracer with custom event store
let tracer = Arc::new(TracerSystem::new(Some(event_store), true));

// Events will trigger the callback
tracer.record_llm_call("llama3.2", vec![], 1.0, None, "test", "corr-1");

println!("Total events: {}", event_count.load(Ordering::SeqCst));
```

## Null Tracer

For environments where tracing should be completely disabled without conditional checks:

```rust
use mojentic::tracer::NullTracer;
use std::collections::HashMap;
use serde_json::json;

let tracer = NullTracer::new();

// All operations are no-ops
tracer.record_llm_call("model", vec![], 1.0, None, "source", "id");
tracer.record_tool_call("tool", HashMap::new(), json!({}), None, None, "source", "id");

// Queries return empty results
assert!(tracer.is_empty());
assert_eq!(tracer.len(), 0);
assert_eq!(tracer.get_event_summaries(None, None, None).len(), 0);
```

## Performance Considerations

1. **Thread Safety**: EventStore uses Arc<Mutex<>> for thread-safe access
2. **Memory**: Events are stored in memory; clear periodically for long-running processes
3. **Overhead**: Minimal when disabled; consider NullTracer for zero overhead
4. **Callbacks**: Keep callback functions fast to avoid blocking event recording

## Integration with LlmBroker

The tracer integrates with `LlmBroker` to automatically record LLM interactions (implementation in progress):

```rust,ignore
use mojentic::llm::{LlmBroker, LlmMessage};
use mojentic::llm::gateways::OllamaGateway;
use mojentic::tracer::TracerSystem;
use std::sync::Arc;

// Create tracer
let tracer = Arc::new(TracerSystem::default());

// Create broker with tracer
let gateway = Arc::new(OllamaGateway::default());
let broker = LlmBroker::builder("llama3.2", gateway)
    .with_tracer(tracer.clone())
    .build();

// Broker will automatically record events
let response = broker.generate(
    &[LlmMessage::user("Hello!")],
    None,
    None
).await?;

// Query tracer for events
let events = tracer.get_event_summaries(None, None, None);
for event in events {
    println!("{}", event);
}
```

## Testing

The tracer system includes comprehensive unit tests covering:

- Event creation and formatting
- Event storage and retrieval
- Callbacks and filtering
- TracerSystem coordination
- NullTracer behavior

Run tests with:
```bash
cargo test tracer
```

##  Implementation Status

**✅ Layer 2 Tracer System - Core Complete**

Current implementation:
- ✅ TracerEvent trait with 4 event types
- ✅ EventStore with callbacks and filtering
- ✅ TracerSystem coordination layer
- ✅ NullTracer for zero-overhead tracing
- ✅ 24 comprehensive unit tests (all passing)
- ✅ Correlation ID support
- ✅ Documentation complete

Pending integration:
- ⏳ LlmBroker integration (add tracer parameter)
- ⏳ Tool system integration (add tracer parameter)
- ⏳ Example application (tracer_demo.rs)

## See Also

- [Observability Overview](./README.md)
- [Error Handling](../core/README.md)
- [LLM Integration](../broker.md)
