# Building Tools

## Creating Custom Tools

Tools extend LLM capabilities by providing functions they can call. All tools must implement the `LlmTool` trait.

### Basic Tool Structure

```rust
use mojentic::prelude::*;
use serde_json::{json, Value};
use std::collections::HashMap;

struct CalculatorTool;

impl LlmTool for CalculatorTool {
    fn run(&self, args: &HashMap<String, Value>) -> mojentic::Result<Value> {
        let operation = args.get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| mojentic::MojenticError::ToolError(
                "Missing operation parameter".to_string()
            ))?;

        let a = args.get("a")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| mojentic::MojenticError::ToolError(
                "Missing 'a' parameter".to_string()
            ))?;

        let b = args.get("b")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| mojentic::MojenticError::ToolError(
                "Missing 'b' parameter".to_string()
            ))?;

        let result = match operation {
            "add" => a + b,
            "subtract" => a - b,
            "multiply" => a * b,
            "divide" => {
                if b == 0.0 {
                    return Err(mojentic::MojenticError::ToolError(
                        "Division by zero".to_string()
                    ));
                }
                a / b
            }
            _ => return Err(mojentic::MojenticError::ToolError(
                format!("Unknown operation: {}", operation)
            )),
        };

        Ok(json!({
            "result": result,
            "operation": operation,
            "a": a,
            "b": b
        }))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "calculator".to_string(),
                description: "Perform basic arithmetic operations".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "operation": {
                            "type": "string",
                            "enum": ["add", "subtract", "multiply", "divide"],
                            "description": "The operation to perform"
                        },
                        "a": {
                            "type": "number",
                            "description": "First operand"
                        },
                        "b": {
                            "type": "number",
                            "description": "Second operand"
                        }
                    },
                    "required": ["operation", "a", "b"]
                }),
            },
        }
    }
}
```

### Using Custom Tools

```rust
#[tokio::main]
async fn main() -> mojentic::Result<()> {
    let gateway = OllamaGateway::new();
    let broker = LlmBroker::new("qwen3:32b", Arc::new(gateway));

    let tools: Vec<Box<dyn LlmTool>> = vec![
        Box::new(CalculatorTool),
    ];

    let messages = vec![
        LlmMessage::user("What is 42 multiplied by 17?")
    ];

    let response = broker.generate(&messages, Some(&tools), None).await?;
    println!("{}", response);

    Ok(())
}
```

## Testing Custom Tools

Create tests for your tools:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculator_add() {
        let tool = CalculatorTool;
        let mut args = HashMap::new();
        args.insert("operation".to_string(), json!("add"));
        args.insert("a".to_string(), json!(5.0));
        args.insert("b".to_string(), json!(3.0));

        let result = tool.run(&args).unwrap();
        assert_eq!(result["result"], 8.0);
    }

    #[test]
    fn test_calculator_divide_by_zero() {
        let tool = CalculatorTool;
        let mut args = HashMap::new();
        args.insert("operation".to_string(), json!("divide"));
        args.insert("a".to_string(), json!(10.0));
        args.insert("b".to_string(), json!(0.0));

        assert!(tool.run(&args).is_err());
    }
}
```

## Best Practices

1. **Error Handling**: Always use proper error types, never panic in library code
2. **Documentation**: Add rustdoc comments to your tool implementations
3. **Testing**: Write comprehensive tests for your tools
4. **Type Safety**: Validate all input parameters
5. **JSON Schema**: Provide clear, detailed parameter descriptions
