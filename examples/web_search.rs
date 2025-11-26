/// Example: Web Search Tool
///
/// This example demonstrates using the WebSearchTool to search
/// the web using DuckDuckGo's lite endpoint.
///
/// Usage:
///   cargo run --example web_search
use mojentic::llm::tools::{web_search_tool::WebSearchTool, LlmTool};
use serde_json::json;
use std::collections::HashMap;

fn main() {
    println!("Web Search Tool Example");
    println!("=======================\n");

    // Create the tool
    let tool = WebSearchTool::new();

    // Example 1: Basic search
    println!("Example 1: Searching for 'Rust programming language'");
    println!("------------------------------------------------------");

    let mut args = HashMap::new();
    args.insert("query".to_string(), json!("Rust programming language"));

    match tool.run(&args) {
        Ok(results) => {
            if let Some(results_array) = results.as_array() {
                println!("Found {} results:\n", results_array.len());

                for (index, result) in results_array.iter().take(5).enumerate() {
                    println!("{}. {}", index + 1, result["title"].as_str().unwrap_or(""));
                    println!("   URL: {}", result["url"].as_str().unwrap_or(""));
                    println!("   Snippet: {}\n", result["snippet"].as_str().unwrap_or(""));
                }
            }
        }
        Err(e) => {
            println!("Search failed: {}", e);
        }
    }

    println!();

    // Example 2: Search with special characters
    println!("Example 2: Searching for 'systems programming & memory safety'");
    println!("----------------------------------------------------------------");

    let mut args = HashMap::new();
    args.insert("query".to_string(), json!("systems programming & memory safety"));

    match tool.run(&args) {
        Ok(results) => {
            if let Some(results_array) = results.as_array() {
                println!("Found {} results:\n", results_array.len());

                for (index, result) in results_array.iter().take(3).enumerate() {
                    println!("{}. {}", index + 1, result["title"].as_str().unwrap_or(""));
                    println!("   URL: {}\n", result["url"].as_str().unwrap_or(""));
                }
            }
        }
        Err(e) => {
            println!("Search failed: {}", e);
        }
    }

    println!();

    // Example 3: Tool descriptor (for LLM integration)
    println!("Example 3: Tool Descriptor for LLM");
    println!("-----------------------------------");

    let descriptor = tool.descriptor();
    println!("Tool Name: {}", descriptor.function.name);
    println!("Description: {}", descriptor.function.description);
    println!("\nParameters:");
    println!("{}", serde_json::to_string_pretty(&descriptor.function.parameters).unwrap());

    println!();

    // Example 4: Error Handling
    println!("Example 4: Error Handling");
    println!("-------------------------");

    let args = HashMap::new(); // Empty args - should fail

    match tool.run(&args) {
        Ok(_) => {
            println!("Unexpected success");
        }
        Err(e) => {
            println!("Expected error: {}", e);
        }
    }

    println!("\n✓ Examples completed!");
}
