# Chat Sessions with Tools

Combine conversational context with tool calling for grounded, actionable responses. `ChatSession` seamlessly integrates with Mojentic's tool system to enable function calling within conversations.

## Overview

When tools are provided to a `ChatSession`, the LLM can call them during the conversation. The broker handles:
1. Detecting tool call requests
2. Executing the appropriate tools
3. Adding tool results to the conversation
4. Continuing the conversation with the LLM

This happens transparently - you just call `send()` as normal.

## Basic Tool Integration

Add tools when building the session:

```rust
use mojentic::llm::{ChatSession, LlmBroker};
use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::tools::simple_date_tool::SimpleDateTool;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gateway = Arc::new(OllamaGateway::default());
    let broker = LlmBroker::new("qwen3:32b", gateway);

    // Create tools
    let tools: Vec<Box<dyn mojentic::llm::LlmTool>> = vec![
        Box::new(SimpleDateTool),
    ];

    // Build session with tools
    let mut session = ChatSession::builder(broker)
        .system_prompt(
            "You are a helpful assistant. When users ask about dates, \
             use the resolve_date tool to convert relative dates to absolute dates."
        )
        .tools(tools)
        .build();

    // The LLM will automatically use the tool when needed
    let response = session.send("What is tomorrow's date?").await?;
    println!("{}", response);

    Ok(())
}
```

## How It Works

1. **User sends a message**: `session.send("What is tomorrow's date?")`
2. **LLM recognizes need for tool**: Generates a tool call request
3. **Broker executes tool**: Calls `SimpleDateTool` with appropriate arguments
4. **Tool result added to history**: As a tool response message
5. **LLM generates final response**: Using the tool's output
6. **Response returned to user**: "Tomorrow's date is 2025-11-14"

All of this happens in a single `send()` call.

## Multiple Tools

Provide multiple tools for different capabilities:

```rust
use mojentic::llm::tools::simple_date_tool::SimpleDateTool;
// Import other tools...

let tools: Vec<Box<dyn mojentic::llm::LlmTool>> = vec![
    Box::new(SimpleDateTool),
    Box::new(CalculatorTool),
    Box::new(WeatherTool),
];

let mut session = ChatSession::builder(broker)
    .system_prompt(
        "You are a helpful assistant with access to date resolution, \
         calculation, and weather information tools. Use them when appropriate."
    )
    .tools(tools)
    .build();
```

The LLM will choose which tool to use based on the user's question.

## Tool Call History

Tool calls and results are part of the conversation history:

```rust
// After a tool call
for msg in session.messages() {
    match msg.role() {
        MessageRole::User => println!("User: {}", msg.content().unwrap_or("")),
        MessageRole::Assistant => {
            if let Some(tool_calls) = &msg.message.tool_calls {
                println!("Assistant called tool: {:?}", tool_calls);
            } else {
                println!("Assistant: {}", msg.content().unwrap_or(""));
            }
        }
        MessageRole::Tool => println!("Tool result: {}", msg.content().unwrap_or("")),
        _ => {}
    }
}
```

## Context Management with Tools

Tool calls and results consume tokens. The session manages this automatically:

```rust
let mut session = ChatSession::builder(broker)
    .max_context(8192)
    .tools(tools)
    .build();

// Even with tool calls, context trimming works
session.send("What's tomorrow?").await?;  // May trigger tool call
session.send("And the day after?").await?;  // May trigger another tool call

// Old messages (including old tool calls) are trimmed when needed
println!("Total tokens: {}", session.total_tokens());
```

## Creating Custom Tools

Build your own tools for specific needs:

```rust
use mojentic::llm::tools::{LlmTool, ToolDescriptor, FunctionDescriptor};
use serde_json::{json, Value};
use std::collections::HashMap;

struct WeatherTool;

impl LlmTool for WeatherTool {
    fn run(&self, args: &HashMap<String, Value>) -> mojentic::error::Result<Value> {
        let city = args.get("city")
            .and_then(|v| v.as_str())
            .ok_or_else(|| mojentic::error::MojenticError::ToolError(
                "Missing city argument".to_string()
            ))?;

        // In reality, you'd call a weather API here
        Ok(json!({
            "city": city,
            "temperature": "72°F",
            "conditions": "Sunny"
        }))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "get_weather".to_string(),
                description: "Get current weather for a city".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "city": {
                            "type": "string",
                            "description": "The city name"
                        }
                    },
                    "required": ["city"]
                }),
            },
        }
    }
}
```

Then use it:

```rust
let tools: Vec<Box<dyn mojentic::llm::LlmTool>> = vec![
    Box::new(WeatherTool),
];

let mut session = ChatSession::builder(broker)
    .system_prompt("You can check weather using the get_weather tool.")
    .tools(tools)
    .build();

let response = session.send("What's the weather in San Francisco?").await?;
```

## Interactive Example

Run the complete example:

```bash
cargo run --example chat_session_with_tool
```

This demonstrates:
- Tool integration in conversation
- Automatic tool invocation
- Natural multi-turn dialogue with tool results

## Best Practices

1. **Clear tool descriptions**: Help the LLM understand when to use each tool
2. **Descriptive system prompts**: Explain what tools are available
3. **Error handling in tools**: Return meaningful errors for invalid inputs
4. **Tool result formatting**: Return structured JSON that's easy for the LLM to interpret
5. **Monitor token usage**: Tools can add significant tokens to the conversation

## Debugging Tool Calls

Enable tracing to see tool execution:

```rust
// Initialize tracing
tracing_subscriber::fmt::init();

// Tool calls will be logged
let response = session.send("What's tomorrow?").await?;
```

You'll see logs like:
```
INFO Tool calls requested: 1
INFO Executing tool: resolve_date
```

## Limitations

- Tool calls count toward the context window
- Very long tool results may need truncation
- Not all models support tool calling equally well
- Tool execution is synchronous (async tools not yet supported)

## Next Steps

- Learn about [Building Tools](building_tools.md)
- Explore [Tool Usage](tool_usage.md) patterns
- Read about [Simple Text Generation](simple_text_generation.md)
