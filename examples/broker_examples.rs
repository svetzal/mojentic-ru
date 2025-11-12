// Comprehensive broker feature tests
//
// This example demonstrates all major broker capabilities:
// - Simple text generation
// - Structured output with schemas
// - Tool usage
// - Image analysis (multimodal)
//
// Usage:
//   cargo run --example broker_examples
//
// Requirements:
//   - Ollama running locally (default: http://localhost:11434)
//   - Models pulled:
//     - qwen3:32b (for text, structured, tools)
//     - qwen3-vl:30b (for image analysis)

use mojentic::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

// Sentiment response structure for structured output
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct SentimentAnalysis {
    label: String,
    confidence: f64,
}

// Simple date tool (mocked for demonstration)
struct SimpleDateTool;

impl LlmTool for SimpleDateTool {
    fn run(&self, _args: &HashMap<String, Value>) -> mojentic::Result<Value> {
        // Return a simple mock result
        Ok(json!({
            "date": "2025-12-25",
            "day_of_week": "Thursday"
        }))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "get_date_info".to_string(),
                description: "Get information about a specific date".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "date": {
                            "type": "string",
                            "description": "The date to get information about"
                        }
                    },
                    "required": ["date"]
                }),
            },
        }
    }
}

fn print_section(title: &str) {
    println!("\n{}", "=".repeat(60));
    println!("  {}", title);
    println!("{}\n", "=".repeat(60));
}

fn print_result(test_name: &str, result: Result<String>) {
    println!("{}:", test_name);
    match result {
        Ok(content) => println!("✅ Success: {}\n", content),
        Err(error) => println!("❌ Error: {}\n", error),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Initialize gateway and brokers
    let gateway = Arc::new(OllamaGateway::new());
    let text_broker = LlmBroker::new("qwen3:32b", gateway.clone());
    let vision_broker = LlmBroker::new("qwen3-vl:30b", gateway.clone());

    // ============================================================================
    // Test 1: Simple Text Generation
    // ============================================================================
    print_section("Test 1: Simple Text Generation");

    println!("Testing with model: qwen3:32b");
    let messages = vec![LlmMessage::user("Hello, how are you?")];

    match text_broker.generate(&messages, None, None).await {
        Ok(response) => print_result("Simple text generation", Ok(response)),
        Err(e) => print_result("Simple text generation", Err(e)),
    }

    // ============================================================================
    // Test 2: Structured Output
    // ============================================================================
    print_section("Test 2: Structured Output");

    println!("Testing structured output with schema...");

    let messages = vec![LlmMessage::user(
        "I love this product! It's amazing and works perfectly.",
    )];

    match text_broker.generate_object::<SentimentAnalysis>(&messages, None).await {
        Ok(result) => print_result(
            "Structured output",
            Ok(format!("label: {}, confidence: {}", result.label, result.confidence)),
        ),
        Err(e) => print_result("Structured output", Err(e)),
    }

    // ============================================================================
    // Test 3: Tool Usage
    // ============================================================================
    print_section("Test 3: Tool Usage");

    println!("Testing tool usage with SimpleDateTool...");

    let date_tool: Box<dyn LlmTool> = Box::new(SimpleDateTool);
    let tools: Vec<Box<dyn LlmTool>> = vec![date_tool];
    let messages = vec![LlmMessage::user("What day of the week is Christmas 2025?")];

    match text_broker.generate(&messages, Some(&tools), None).await {
        Ok(response) => print_result("Tool usage", Ok(response)),
        Err(e) => print_result("Tool usage", Err(e)),
    }

    // ============================================================================
    // Test 4: Image Analysis (Multimodal)
    // ============================================================================
    print_section("Test 4: Image Analysis (Multimodal)");

    // Get the absolute path to the image
    let mut image_path = std::env::current_dir()?;
    image_path.push("examples");
    image_path.push("images");
    image_path.push("flash_rom.jpg");

    if image_path.exists() {
        println!("Testing image analysis with model: qwen3-vl:30b");
        println!("Image path: {}", image_path.display());

        let messages = vec![LlmMessage::user(
            "What text is visible in this image? Please extract all readable text.",
        )
        .with_images(vec![image_path.to_string_lossy().to_string()])];

        match vision_broker.generate(&messages, None, None).await {
            Ok(response) => print_result("Image analysis", Ok(response)),
            Err(e) => print_result("Image analysis", Err(e)),
        }
    } else {
        println!("❌ Image file not found: {}", image_path.display());
        println!("Skipping image analysis test.\n");
    }

    // ============================================================================
    // Summary
    // ============================================================================
    print_section("Summary");

    println!(
        r#"
All broker feature tests completed!

Features demonstrated:
✓ Simple text generation
✓ Structured output with JSON schema
✓ Tool calling with DateResolver
✓ Multimodal image analysis

For more detailed examples, see:
- examples/simple_llm.rs
- examples/structured_output.rs
- examples/tool_usage.rs
- examples/image_analysis.rs
"#
    );

    Ok(())
}
