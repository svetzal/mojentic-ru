use mojentic::prelude::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

// Example tool that gets the current weather (mocked)
struct GetWeatherTool;

impl LlmTool for GetWeatherTool {
    fn run(&self, args: &HashMap<String, Value>) -> mojentic::Result<Value> {
        let location = args
            .get("location")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        // In a real implementation, you would call a weather API here
        Ok(json!({
            "location": location,
            "temperature": 22,
            "condition": "sunny",
            "humidity": 60
        }))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "get_weather".to_string(),
                description: "Get the current weather for a location".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "location": {
                            "type": "string",
                            "description": "The city or location to get weather for"
                        }
                    },
                    "required": ["location"]
                }),
            },
        }
    }
}

#[tokio::main]
async fn main() -> mojentic::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create Ollama gateway
    let gateway = OllamaGateway::new();

    // Create broker with a local model
    let broker = LlmBroker::new("qwen3:32b", Arc::new(gateway));

    // Create tools
    let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(GetWeatherTool)];

    // Create a message that should trigger tool use
    let messages = vec![LlmMessage::user(
        "What's the weather like in San Francisco?",
    )];

    // Generate a response (the LLM should call the tool)
    println!("Asking about weather (this will use the tool)...");
    let response = broker.generate(&messages, Some(&tools), None).await?;

    println!("\nResponse: {}", response);

    Ok(())
}
