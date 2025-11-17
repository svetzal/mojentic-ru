# Async Agent System

The async agent framework provides a foundation for building reactive, event-driven systems where agents process events asynchronously and can emit new events in response.

## Core Concepts

### Events

Events are the fundamental unit of communication between agents. Each event has:
- A `source` - the agent that created the event
- An optional `correlation_id` - to track related events through a workflow
- Custom data fields specific to the event type

```rust
use mojentic::event::Event;
use serde::{Deserialize, Serialize};
use std::any::Any;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QuestionEvent {
    source: String,
    correlation_id: Option<String>,
    question: String,
}

impl Event for QuestionEvent {
    fn source(&self) -> &str {
        &self.source
    }

    fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }

    fn set_correlation_id(&mut self, id: String) {
        self.correlation_id = Some(id);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }
}
```

### Base Async Agent

All agents implement the `BaseAsyncAgent` trait:

```rust
use mojentic::agents::BaseAsyncAgent;
use mojentic::event::Event;
use mojentic::Result;
use async_trait::async_trait;

struct MyAgent;

#[async_trait]
impl BaseAsyncAgent for MyAgent {
    async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
        // Process the event and return new events
        Ok(vec![])
    }
}
```

### Async LLM Agent

The `AsyncLlmAgent` uses an LLM to process events:

```rust
use mojentic::agents::AsyncLlmAgent;
use mojentic::llm::LlmBroker;
use std::sync::Arc;

let broker = Arc::new(LlmBroker::new("model-name", gateway, None));
let agent = AsyncLlmAgent::new(
    broker,
    "You are a helpful assistant.",
    None, // optional tools
);
```

You can generate both text and structured responses:

```rust
// Generate text
let response = agent.generate_response("Hello", None).await?;

// Generate structured object
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
struct Response {
    answer: String,
    confidence: f64,
}

let response: Response = agent.generate_object("Question", None).await?;
```

### Async Aggregator Agent

The `AsyncAggregatorAgent` collects multiple events before processing them:

```rust
use mojentic::agents::AsyncAggregatorAgent;
use std::any::TypeId;

let agent = AsyncAggregatorAgent::new(vec![
    TypeId::of::<Event1>(),
    TypeId::of::<Event2>(),
]);

// Wait for all needed events
let events = agent.wait_for_events("correlation-id", Some(Duration::from_secs(30))).await?;
```

Override the `process_events` method to handle the collected events:

```rust
#[async_trait]
impl BaseAsyncAgent for MyAggregator {
    async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
        // Delegate to aggregator for collection
        if let Some(events) = self.aggregator.capture_event(event).await? {
            // All events collected, process them
            return self.process_collected_events(events).await;
        }
        Ok(vec![])
    }
}
```

### Router

The `Router` maps event types to agents:

```rust
use mojentic::router::Router;
use std::sync::Arc;

let mut router = Router::new();
router.add_route::<QuestionEvent>(my_agent);
router.add_route::<QuestionEvent>(another_agent); // Multiple agents can handle same event
```

### Async Dispatcher

The `AsyncDispatcher` manages event processing in a background task:

```rust
use mojentic::async_dispatcher::AsyncDispatcher;
use std::time::Duration;

let mut dispatcher = AsyncDispatcher::new(Arc::new(router));
dispatcher.start().await?;

dispatcher.dispatch(my_event);

// Wait for processing to complete
dispatcher.wait_for_empty_queue(Some(Duration::from_secs(10))).await?;

dispatcher.stop().await?;
```

## Complete Example

See `examples/async_llm.rs` for a complete example demonstrating:

1. **Multiple LLM Agents** - FactCheckerAgent and AnswerGeneratorAgent process questions independently
2. **Event Aggregation** - FinalAnswerAgent waits for both agents' responses before combining them
3. **Correlation Tracking** - Events are tracked through the entire workflow via correlation_id
4. **Async Dispatch** - All processing happens asynchronously in the background

```bash
cargo run --example async_llm
```

## Key Async Patterns Used

### Arc and Mutex for Shared State

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

let shared_data: Arc<Mutex<HashMap<String, Vec<Event>>>> = Arc::new(Mutex::new(HashMap::new()));
```

### Oneshot Channels for Notifications

```rust
use tokio::sync::oneshot;

let (tx, rx) = oneshot::channel();
// Send notification
let _ = tx.send(data);
// Wait for notification
let data = rx.await?;
```

### Timeout Handling

```rust
use tokio::time::{timeout, Duration};

match timeout(Duration::from_secs(30), operation).await {
    Ok(result) => result,
    Err(_) => return Err(MojenticError::TimeoutError("Operation timed out".to_string())),
}
```

### Background Tasks

```rust
use tokio::task::JoinHandle;

let handle: JoinHandle<()> = tokio::spawn(async move {
    // Background work
});

// Later, wait for completion
handle.await?;
```

## Design Principles

### Clarity over Cleverness

The code prioritizes readability and explicit behavior. Agent interactions are clear and traceable.

### Composability

Agents are independent units that can be composed into complex workflows. The event system decouples agents from each other.

### Type Safety

TypeId is used to route events to the correct agents. The compiler ensures type safety at the routing layer.

### Functional Core, Imperative Shell

Agents have minimal side effects. The dispatcher handles I/O and orchestration at the boundary.

## Testing

All components have comprehensive unit tests:

- **BaseAsyncAgent tests** - Agent trait implementation
- **AsyncLlmAgent tests** - LLM integration with mocked gateways
- **AsyncAggregatorAgent tests** - Event collection and timeout behavior
- **AsyncDispatcher tests** - Event routing and background processing
- **Router tests** - Type-based routing logic

Run tests with:

```bash
cargo test --lib
```

Test coverage is measured with tarpaulin:

```bash
cargo tarpaulin --lib
```
