# Working Memory Pattern

The working memory pattern enables agents to maintain and share context across multiple interactions. This guide shows how to use `SharedWorkingMemory` and build memory-aware agents in Rust.

## Overview

Working memory provides:
- **Shared Context**: Multiple agents can read from and write to the same memory
- **Continuous Learning**: Agents automatically learn and remember new information
- **State Persistence**: Knowledge is maintained across interactions
- **Thread Safety**: Safe concurrent access using `Arc<Mutex<T>>`

## Quick Start

### Basic Usage

```rust
use mojentic::context::SharedWorkingMemory;
use serde_json::json;

// Create memory with initial data
let memory = SharedWorkingMemory::new(json!({
    "User": {
        "name": "Alice",
        "age": 30
    }
}));

// Retrieve current state
let current = memory.get_working_memory();

// Update memory (deep merge)
memory.merge_to_working_memory(&json!({
    "User": {
        "city": "NYC",
        "preferences": {
            "theme": "dark"
        }
    }
}));

// Result: {"User": {"name": "Alice", "age": 30, "city": "NYC", "preferences": {...}}}
```

### Memory-Aware Agent Pattern

```rust
use mojentic::llm::{LlmBroker, LlmMessage};
use mojentic::llm::gateways::OllamaGateway;
use mojentic::context::SharedWorkingMemory;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize
    let gateway = OllamaGateway::default();
    let broker = LlmBroker::new("qwen2.5:7b", gateway);
    let memory = SharedWorkingMemory::new(json!({
        "User": {"name": "Alice"}
    }));

    // Create prompt with memory context
    let memory_context = memory.get_working_memory();
    let prompt = format!(
        "This is what you remember: {}\n\nRemember anything new you learn.\n\nUser: I love pizza",
        serde_json::to_string_pretty(&memory_context)?
    );

    // Generate response with schema that includes memory field
    let schema = json!({
        "type": "object",
        "required": ["answer"],
        "properties": {
            "answer": {"type": "string"},
            "memory": {
                "type": "object",
                "description": "Add anything new you learned here."
            }
        }
    });

    let response = broker.generate(
        vec![LlmMessage::user(&prompt)],
        Some(schema),
        None,
        None,
        None,
        None
    ).await?;

    // Parse response and update memory
    let response_json: serde_json::Value = serde_json::from_str(&response.content)?;
    if let Some(learned) = response_json.get("memory") {
        memory.merge_to_working_memory(learned);
    }

    Ok(())
}
```

## Core Concepts

### SharedWorkingMemory

A thread-safe, mutable key-value store that agents use to share context:

```rust
pub struct SharedWorkingMemory {
    memory: Arc<Mutex<HashMap<String, Value>>>,
}

impl SharedWorkingMemory {
    pub fn new(initial: Value) -> Self
    pub fn get_working_memory(&self) -> Value
    pub fn merge_to_working_memory(&self, updates: &Value)
}
```

**Key features:**
- **Thread-Safe**: Uses `Arc<Mutex<T>>` for concurrent access
- **Deep Merge**: Nested JSON objects are recursively merged
- **Simple API**: Just 3 methods to learn

## Deep Merge Behavior

Memory updates use deep merge to preserve existing data:

```rust
use serde_json::json;

// Initial memory
let memory = SharedWorkingMemory::new(json!({
    "User": {
        "name": "Alice",
        "age": 30,
        "address": {
            "city": "NYC",
            "state": "NY"
        }
    }
}));

// Update with nested data
memory.merge_to_working_memory(&json!({
    "User": {
        "age": 31,
        "address": {
            "zip": "10001"
        }
    }
}));

// Result: All fields preserved, nested objects merged
// {
//   "User": {
//     "name": "Alice",      // Preserved
//     "age": 31,            // Updated
//     "address": {
//       "city": "NYC",      // Preserved
//       "state": "NY",      // Preserved
//       "zip": "10001"      // Added
//     }
//   }
// }
```

## Building Memory-Aware Agents

Here's a complete pattern for memory-aware agents:

```rust
use mojentic::llm::{LlmBroker, LlmMessage};
use mojentic::context::SharedWorkingMemory;
use mojentic::agents::{Event, BaseAsyncAgent};
use serde::{Deserialize, Serialize};
use serde_json::json;
use async_trait::async_trait;

#[derive(Serialize, Deserialize)]
struct ResponseModel {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory: Option<serde_json::Value>,
}

struct MemoryAgent {
    broker: Arc<LlmBroker>,
    memory: SharedWorkingMemory,
    behaviour: String,
    instructions: String,
}

impl MemoryAgent {
    fn new(
        broker: Arc<LlmBroker>,
        memory: SharedWorkingMemory,
        behaviour: String,
        instructions: String,
    ) -> Self {
        Self { broker, memory, behaviour, instructions }
    }

    async fn generate_with_memory(
        &self,
        user_input: &str,
    ) -> Result<ResponseModel, Box<dyn std::error::Error>> {
        // Build prompt with memory context
        let memory_context = self.memory.get_working_memory();
        let messages = vec![
            LlmMessage::system(&self.behaviour),
            LlmMessage::user(&format!(
                "This is what you remember:\n{}\n\n{}",
                serde_json::to_string_pretty(&memory_context)?,
                self.instructions
            )),
            LlmMessage::user(user_input),
        ];

        // Schema with memory field
        let schema = json!({
            "type": "object",
            "required": ["text"],
            "properties": {
                "text": {"type": "string"},
                "memory": {
                    "type": "object",
                    "description": "Add new information here."
                }
            }
        });

        // Generate response
        let response = self.broker.generate(
            messages,
            Some(schema),
            None,
            None,
            None,
            None
        ).await?;

        // Parse and update memory
        let result: ResponseModel = serde_json::from_str(&response.content)?;
        if let Some(ref learned) = result.memory {
            self.memory.merge_to_working_memory(learned);
        }

        Ok(result)
    }
}
```

## Multi-Agent Coordination

Multiple agents can share the same memory instance:

```rust
use std::sync::Arc;

// Shared memory
let memory = Arc::new(SharedWorkingMemory::new(json!({
    "context": {}
})));

// Multiple agents
let researcher = MemoryAgent::new(
    Arc::clone(&broker),
    Arc::clone(&memory),
    "You are a research assistant.".to_string(),
    "Research topics thoroughly.".to_string(),
);

let writer = MemoryAgent::new(
    Arc::clone(&broker),
    Arc::clone(&memory),
    "You are a technical writer.".to_string(),
    "Write clear documentation.".to_string(),
);

// Researcher updates memory
let research = researcher
    .generate_with_memory("Research Rust async patterns")
    .await?;

// Writer uses updated memory (already shared)
let article = writer
    .generate_with_memory("Write an article about what was researched")
    .await?;
```

## Use Cases

### 1. Conversational Chatbots

```rust
let memory = SharedWorkingMemory::new(json!({
    "conversation_history": [],
    "user_preferences": {}
}));
```

### 2. Workflow Automation

```rust
let memory = SharedWorkingMemory::new(json!({
    "workflow_state": "started",
    "completed_steps": [],
    "pending_tasks": []
}));
```

### 3. Knowledge Base Building

```rust
let memory = SharedWorkingMemory::new(json!({
    "entities": {},
    "relationships": [],
    "facts": []
}));
```

## Best Practices

### 1. Structure Your Memory

Use clear, hierarchical keys:

```rust
json!({
    "User": {...},
    "Conversation": {...},
    "SystemState": {...}
})
```

### 2. Use Arc for Sharing

Share memory across threads/agents:

```rust
let memory = Arc::new(SharedWorkingMemory::new(initial));
let memory_clone = Arc::clone(&memory);
```

### 3. Validate Memory Updates

Check memory quality before accepting:

```rust
if let Some(ref learned) = result.memory {
    if is_valid_update(learned) {
        memory.merge_to_working_memory(learned);
    }
}
```

### 4. Handle Errors Gracefully

Memory operations can fail:

```rust
match agent.generate_with_memory(input).await {
    Ok(response) => {
        // Process response
    }
    Err(e) => {
        eprintln!("Failed to generate response: {}", e);
        // Don't update memory on error
    }
}
```

## Example Application

See the complete working memory example:

```bash
cd mojentic-ru
cargo run --example working_memory
```

The example demonstrates:
- Initializing memory with user data
- RequestAgent that learns from conversation
- Event-driven coordination with AsyncDispatcher
- Memory persistence across interactions

## API Reference

### SharedWorkingMemory

```rust
impl SharedWorkingMemory {
    /// Create new memory with initial data
    pub fn new(initial: Value) -> Self

    /// Get current memory snapshot
    pub fn get_working_memory(&self) -> Value

    /// Deep merge updates into memory
    pub fn merge_to_working_memory(&self, updates: &Value)
}
```

See `src/context/shared_working_memory.rs` for full documentation.

## Thread Safety

`SharedWorkingMemory` is thread-safe and can be shared across async tasks:

```rust
use tokio::task;

let memory = Arc::new(SharedWorkingMemory::new(initial));

let task1 = {
    let memory = Arc::clone(&memory);
    task::spawn(async move {
        memory.merge_to_working_memory(&updates1);
    })
};

let task2 = {
    let memory = Arc::clone(&memory);
    task::spawn(async move {
        memory.merge_to_working_memory(&updates2);
    })
};

task1.await?;
task2.await?;
```
