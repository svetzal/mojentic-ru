# Tutorial: Building Chatbots

## Why Use Chat Sessions?

When working with Large Language Models (LLMs), simple text generation is useful for one-off interactions, but many applications require ongoing conversations where the model remembers previous exchanges. This is where chat sessions come in.

Chat sessions are essential when you need to:

- Build conversational agents or chatbots
- Maintain context across multiple user interactions
- Create applications where the LLM needs to remember previous information
- Develop more natural and coherent conversational experiences

## When to Apply This Approach

Use chat sessions when:

- Your application requires multi-turn conversations
- You need the LLM to reference information from earlier in the conversation
- You want to create a more interactive and engaging user experience

## The Key Difference: Expanding Context

The fundamental difference between simple text generation and chat sessions is the **expanding context**. With each new message in a chat session:

1. The message is added to the conversation history
2. All previous messages (within token limits) are sent to the LLM with each new query
3. The LLM can reference and build upon earlier parts of the conversation

## Getting Started

Let's walk through a simple example of building a chatbot using Mojentic's `ChatSession`.

### Basic Implementation

Here's the simplest way to create a chat session with Mojentic:

```rust
use mojentic::prelude::*;
use std::io::{self, Write};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Create an LLM broker
    let gateway = Arc::new(OllamaGateway::new());
    let broker = LlmBroker::new("qwen3:32b", gateway, None);

    // 2. Initialize a chat session
    let mut session = ChatSession::new(broker);

    // 3. Simple interactive loop
    println!("Chatbot started. Type 'exit' to quit.");

    loop {
        print!("Query: ");
        io::stdout().flush()?;

        let mut query = String::new();
        io::stdin().read_line(&mut query)?;
        let query = query.trim();

        if query == "exit" {
            break;
        }

        let response = session.send_message(query).await?;
        println!("{}", response);
    }
    
    Ok(())
}
```

This code creates an interactive chatbot that maintains context across multiple exchanges.

## Step-by-Step Explanation

### 1. Initialize the Broker

```rust
let gateway = Arc::new(OllamaGateway::new());
let broker = LlmBroker::new("qwen3:32b", gateway, None);
```

The `LlmBroker` is the central component that handles communication with the LLM provider (in this case, Ollama).

### 2. Start the Session

```rust
let mut session = ChatSession::new(broker);
```

`ChatSession` holds the state of the conversation. By default, it manages the message history and ensures it fits within the model's context window.

### 3. Send Messages

```rust
let response = session.send_message(query).await?;
```

When you send a message:
1. It's added to the history.
2. The full history is sent to the LLM.
3. The LLM's response is added to the history.
4. The response text is returned.

## Customizing Your Chat Session

You can customize the session with a system prompt or tools.

### System Prompt

The system prompt sets the behavior of the assistant.

```rust
let mut session = ChatSession::new(broker)
    .with_system_prompt("You are a helpful AI assistant specialized in Rust programming.");
```

### Adding Tools

You can enhance your chatbot by providing tools that the LLM can use.

```rust
use mojentic::tools::DateResolver;

let tools: Vec<Arc<dyn LlmTool>> = vec![Arc::new(DateResolver::new())];

let mut session = ChatSession::new(broker)
    .with_tools(tools);

// The LLM can now use the date tool in conversations
let response = session.send_message("What day of the week is July 4th, 2025?").await?;
println!("{}", response);
```

## Summary

In this tutorial, we've learned how to:
1.  Initialize a `ChatSession` with an `LlmBroker`.
2.  Create an interactive loop to chat with the model.
3.  Customize the session with system prompts and tools.

By leveraging chat sessions, you can create engaging conversational experiences that maintain context across multiple interactions.
