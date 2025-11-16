use mojentic::llm::tools::current_datetime_tool::CurrentDatetimeTool;
use mojentic::prelude::*;
use std::sync::Arc;

#[tokio::main]
async fn main() -> mojentic::Result<()> {
    println!("🚀 Mojentic Rust - Current Datetime Tool Example\n");

    // Create Ollama gateway
    let gateway = OllamaGateway::new();

    // Create broker with a local model
    let broker = LlmBroker::new("qwen3:32b", Arc::new(gateway), None);

    // Create the tool
    let tool = CurrentDatetimeTool::new();

    println!("Available tool:");
    let descriptor = tool.descriptor();
    println!("  - {}: {}", descriptor.function.name, descriptor.function.description);
    println!();

    // Example 1: Ask for current time
    println!("Example 1: What time is it right now?\n");

    let messages = vec![
        LlmMessage::system("You are a helpful assistant with access to tools."),
        LlmMessage::user("What time is it right now? Also, what day of the week is it today?"),
    ];

    let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(tool)];

    match broker.generate(&messages, Some(&tools), None, None).await {
        Ok(response) => {
            println!("LLM Response:");
            println!("{}", response);
            println!();
        }
        Err(e) => {
            println!("Error: {}\n", e);
        }
    }

    // Example 2: Ask for current date in a friendly format
    println!("Example 2: What's today's date in a friendly format?\n");

    let messages = vec![
        LlmMessage::system("You are a helpful assistant with access to tools."),
        LlmMessage::user(
            "Tell me the current date in a friendly format, like 'Monday, January 1, 2023'",
        ),
    ];

    match broker.generate(&messages, Some(&tools), None, None).await {
        Ok(response) => {
            println!("LLM Response:");
            println!("{}", response);
            println!();
        }
        Err(e) => {
            println!("Error: {}\n", e);
        }
    }

    // Example 3: Multiple queries about time
    println!("Example 3: When was this program run?\n");

    let messages = vec![
        LlmMessage::system("You are a helpful assistant with access to tools."),
        LlmMessage::user("When was this program run? Give me the exact timestamp."),
    ];

    match broker.generate(&messages, Some(&tools), None, None).await {
        Ok(response) => {
            println!("LLM Response:");
            println!("{}", response);
            println!();
        }
        Err(e) => {
            println!("Error: {}\n", e);
        }
    }

    println!("✅ Example completed!");

    Ok(())
}
