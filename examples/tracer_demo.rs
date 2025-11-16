//! Interactive chat demonstration with tracer system
//!
//! This example shows how to use the tracer system to monitor an interactive
//! chat session with LlmBroker and tools. When the user exits the session,
//! the script displays a comprehensive summary of all traced events.
//!
//! It demonstrates how correlation_id is used to trace related events across
//! the system, allowing you to track the flow of a request from start to finish.
//!
//! # Running the example
//!
//! ```bash
//! cargo run --example tracer_demo
//! ```
//!
//! Make sure you have Ollama running locally with an appropriate model.

use mojentic::llm::broker::LlmBroker;
use mojentic::llm::chat_session::ChatSession;
use mojentic::llm::gateways::ollama::OllamaGateway;
use mojentic::llm::models::LlmMessage;
use mojentic::llm::tools::simple_date_tool::SimpleDateTool;
use mojentic::llm::tools::LlmTool;
use mojentic::tracer::TracerSystem;
use std::io::{self, Write};
use std::sync::Arc;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=".repeat(80));
    println!("Interactive Chat with Tracer Demonstration");
    println!("=".repeat(80));
    println!();
    println!("This example demonstrates the tracer system for observability.");
    println!("Ask questions about dates (e.g., 'What day is tomorrow?') or anything else.");
    println!("Behind the scenes, the tracer records all LLM calls, responses, and tool executions.");
    println!("Each conversation turn is assigned a unique correlation_id to trace related events.");
    println!();
    println!("Press Enter with no input to exit and see the trace summary.");
    println!("=".repeat(80));
    println!();

    // Create tracer system for monitoring
    let tracer = Arc::new(TracerSystem::default());

    // Create LLM gateway (using Ollama locally)
    let gateway = Arc::new(OllamaGateway::with_host("http://localhost:11434"));

    // Create broker with tracer
    let broker = LlmBroker::new(
        "qwen2.5:7b", // Adjust model name as needed
        gateway,
        Some(tracer.clone()),
    );

    // Create tools
    let date_tool = SimpleDateTool;
    let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(date_tool)];

    // We'll use the broker directly instead of ChatSession to pass correlation_id
    // In a production system, ChatSession could be extended to accept correlation_id

    // Track correlation IDs for each conversation turn
    let mut turn_counter = 0;
    let mut conversation_correlation_ids = Vec::new();

    // System prompt for context
    let system_prompt =
        "You are a helpful assistant. When asked about dates, use the resolve_date tool.";

    // Interactive chat loop
    let mut conversation_history: Vec<LlmMessage> = vec![LlmMessage::system(system_prompt)];

    loop {
        print!("\nYou: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            println!("\nExiting chat session...");
            break;
        }

        // Generate unique correlation ID for this turn
        let correlation_id = Uuid::new_v4().to_string();
        turn_counter += 1;
        conversation_correlation_ids.push((turn_counter, correlation_id.clone()));

        println!(
            "[Turn {}, correlation_id: {}...]",
            turn_counter,
            &correlation_id[..8]
        );

        // Add user message to conversation
        conversation_history.push(LlmMessage::user(input));

        print!("Assistant: ");
        io::stdout().flush()?;

        match broker
            .generate(
                &conversation_history,
                Some(&tools),
                None,
                Some(correlation_id),
            )
            .await
        {
            Ok(response) => {
                println!("{}", response);
                // Add assistant response to history
                conversation_history.push(LlmMessage::assistant(&response));
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }
    }

    // Display tracer summary after exit
    display_tracer_summary(&tracer, &conversation_correlation_ids);

    Ok(())
}

/// Display comprehensive tracer event summary
fn display_tracer_summary(tracer: &TracerSystem, correlation_ids: &[(usize, String)]) {
    println!("\n");
    println!("=".repeat(80));
    println!("Tracer System Summary");
    println!("=".repeat(80));
    println!();

    let all_events = tracer.get_event_summaries(None, None, None);
    println!("Total events recorded: {}", all_events.len());
    println!();

    // Count events by type
    let llm_call_count = tracer.count_events(None, None, Some(&|e| {
        e.printable_summary().contains("LlmCallTracerEvent")
    }));
    let llm_response_count = tracer.count_events(None, None, Some(&|e| {
        e.printable_summary().contains("LlmResponseTracerEvent")
    }));
    let tool_call_count = tracer.count_events(None, None, Some(&|e| {
        e.printable_summary().contains("ToolCallTracerEvent")
    }));

    println!("Events by type:");
    println!("  - LLM Call Events: {}", llm_call_count);
    println!("  - LLM Response Events: {}", llm_response_count);
    println!("  - Tool Call Events: {}", tool_call_count);
    println!();

    // Display all events
    println!("{}", "-".repeat(80));
    println!("All Tracer Events:");
    println!("{}", "-".repeat(80));
    for (i, event) in all_events.iter().enumerate() {
        println!("{}. {}", i + 1, event);
        println!();
    }

    // Show events for the first conversation turn if any
    if let Some((turn, correlation_id)) = correlation_ids.first() {
        println!("{}", "-".repeat(80));
        println!(
            "Events for Conversation Turn {} (correlation_id: {}...):",
            turn,
            &correlation_id[..8]
        );
        println!("{}", "-".repeat(80));

        let filter = |e: &dyn mojentic::tracer::TracerEvent| e.correlation_id() == correlation_id;
        let turn_events = tracer.get_event_summaries(None, None, Some(&filter));

        if turn_events.is_empty() {
            println!("No events found with this correlation_id.");
        } else {
            println!(
                "Found {} related event(s) for this conversation turn:",
                turn_events.len()
            );
            println!();
            for (i, event) in turn_events.iter().enumerate() {
                println!("{}. {}", i + 1, event);
                println!();
            }

            println!("The correlation_id allows you to trace the complete flow:");
            println!("  1. Initial LLM call with user message");
            println!("  2. LLM response (possibly with tool calls)");
            println!("  3. Tool execution(s) if requested");
            println!("  4. Follow-up LLM call(s) with tool results");
            println!();
            println!("This creates a complete audit trail for debugging and observability.");
        }
    }

    // Show tool usage statistics if any tool calls were made
    if tool_call_count > 0 {
        println!();
        println!("{}", "-".repeat(80));
        println!("Tool Usage Statistics:");
        println!("{}", "-".repeat(80));

        // Extract tool names from event summaries
        let mut tool_usage = std::collections::HashMap::new();
        for event in &all_events {
            if event.contains("ToolCallTracerEvent") {
                // Simple parsing - in production you'd have more sophisticated analysis
                if event.contains("resolve_date") {
                    *tool_usage.entry("resolve_date").or_insert(0) += 1;
                }
            }
        }

        if tool_usage.is_empty() {
            println!("No tool usage data available.");
        } else {
            for (tool_name, count) in tool_usage.iter() {
                println!("  - {}: {} call(s)", tool_name, count);
            }
        }
    }

    println!();
    println!("=".repeat(80));
    println!("Tracer demonstration complete!");
    println!("=".repeat(80));
}
