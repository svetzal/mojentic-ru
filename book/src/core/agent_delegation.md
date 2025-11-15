# Agent Delegation with ToolWrapper

The ToolWrapper enables agent delegation patterns by wrapping an agent (broker + tools + behavior) as a tool that can be used by other agents. This allows you to build hierarchical agent systems where coordinator agents can delegate specialized tasks to expert agents.

## Overview

ToolWrapper wraps three core components:
- **Broker**: The LLM interface for the agent
- **Tools**: The tools available to the agent
- **Behavior**: The system message defining the agent's personality and capabilities

The wrapped agent appears as a standard tool with a single `input` parameter, making it easy for other agents to delegate work.

## Basic Usage

```rust
use mojentic::llm::{LlmBroker, LlmTool, ToolWrapper};
use mojentic::llm::gateways::OllamaGateway;
use std::sync::Arc;

// Create a specialist agent
let gateway = Arc::new(OllamaGateway::default());
let specialist_broker = Arc::new(LlmBroker::new("qwen2.5:7b", gateway.clone()));

let specialist_tools: Vec<Box<dyn LlmTool>> = vec![
    // Add specialist's tools here
];

let specialist_behaviour =
    "You are a specialist in temporal reasoning and date calculations.";

// Wrap the specialist as a tool
let specialist_tool = ToolWrapper::new(
    specialist_broker,
    specialist_tools,
    specialist_behaviour,
    "temporal_specialist",
    "A specialist that handles date and time-related queries."
);

// Use the specialist in a coordinator agent
let coordinator_broker = LlmBroker::new("qwen2.5:14b", gateway);
let coordinator_tools: Vec<Box<dyn LlmTool>> = vec![
    Box::new(specialist_tool)
];

// The coordinator can now delegate to the specialist
```

## How It Works

1. **Tool Descriptor**: The ToolWrapper generates a tool descriptor with:
   - Function name (e.g., "temporal_specialist")
   - Description (what the agent does)
   - Single `input` parameter for instructions

2. **Execution Flow**: When called:
   - Extracts the `input` parameter
   - Creates initial messages with the agent's behavior (system message)
   - Appends the input as a user message
   - Calls the agent's broker with its tools
   - Returns the agent's response

3. **Delegation Pattern**: The coordinator agent sees specialist agents as tools and decides when to delegate based on the task requirements.

## Example: Multi-Agent System

See `examples/broker_as_tool.rs` for a complete example demonstrating:
- Creating specialist agents with specific tools and behaviors
- Wrapping specialists as tools
- Building a coordinator that delegates appropriately
- Testing different query types

```bash
cargo run --example broker_as_tool
```

## Design Considerations

### Agent Ownership
ToolWrapper uses `Arc<LlmBroker>` to handle shared ownership of the broker. This allows multiple references to the same broker if needed.

### Async Execution
The `run` method is synchronous (required by the `LlmTool` trait) but internally handles async operations using `tokio::task::block_in_place`. This requires tests to use the multi-threaded runtime:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn test_tool_wrapper() {
    // Test code here
}
```

### Tool Isolation
Each wrapped agent maintains its own:
- Tool set
- Behavior/personality
- Conversation context (per invocation)

This ensures clean separation between agents and prevents context bleeding.

## Common Patterns

### Specialist Agent Pattern
Create focused agents with specific expertise:

```rust
// Data analysis specialist
let data_analyst = ToolWrapper::new(
    analyst_broker,
    vec![Box::new(DataQueryTool), Box::new(VisualizationTool)],
    "You are a data analyst specializing in statistical analysis.",
    "data_analyst",
    "Analyzes data and provides statistical insights."
);

// Writing specialist
let writer = ToolWrapper::new(
    writer_broker,
    vec![Box::new(GrammarCheckTool), Box::new(StyleGuideTool)],
    "You are a professional writer and editor.",
    "writer",
    "Improves and edits written content."
);
```

### Coordinator Pattern
Build a coordinator that orchestrates multiple specialists:

```rust
let coordinator = LlmBroker::new("qwen2.5:32b", gateway);
let tools: Vec<Box<dyn LlmTool>> = vec![
    Box::new(data_analyst),
    Box::new(writer),
    Box::new(researcher),
];

// Coordinator decides which specialist to use based on the task
```

### Hierarchical Agents
Create multi-level hierarchies:

```rust
// Level 1: Base specialists
let math_specialist = ToolWrapper::new(...);
let physics_specialist = ToolWrapper::new(...);

// Level 2: Domain coordinator
let science_coordinator = ToolWrapper::new(
    coordinator_broker,
    vec![Box::new(math_specialist), Box::new(physics_specialist)],
    "You coordinate math and physics specialists.",
    "science_coordinator",
    "Handles scientific queries."
);

// Level 3: Top-level coordinator
let main_coordinator = LlmBroker::new("qwen2.5:32b", gateway);
let main_tools: Vec<Box<dyn LlmTool>> = vec![
    Box::new(science_coordinator),
    // Other domain coordinators...
];
```

## Best Practices

1. **Clear Specialization**: Give each agent a well-defined area of expertise
2. **Descriptive Names**: Use clear, descriptive names for tools (e.g., "temporal_specialist", not "agent1")
3. **Comprehensive Descriptions**: Write detailed descriptions so coordinators understand when to delegate
4. **Model Selection**: Use appropriate model sizes for each role:
   - Specialists: Smaller, faster models (7B-14B)
   - Coordinators: Larger, more capable models (32B+)
5. **Tool Composition**: Specialists should have tools relevant to their domain
6. **Testing**: Test both individual specialists and the full delegation chain

## Limitations

- **Synchronous Interface**: The `LlmTool` trait requires synchronous `run` methods, though async operations happen internally
- **No Streaming**: Tool calls don't support streaming responses (limitation of current tool architecture)
- **Context**: Each tool invocation is independent; no automatic context sharing between calls
- **Error Handling**: Tool errors propagate up to the coordinator; implement appropriate error handling

## See Also

- [Tool Usage](tool_usage.md) - General tool usage patterns
- [Building Tools](building_tools.md) - Creating custom tools
- [Chat Sessions with Tools](chat_sessions_with_tools.md) - Using tools in chat sessions
