# Tool Usage

Tools allow LLMs to perform actions, retrieve information, and interact with external systems using strongly-typed Rust functions.

## What are Tools?

Tools extend LLM capabilities:

- **Information Retrieval**: Fetch current data
- **Computations**: Perform calculations
- **System Interactions**: File operations, commands
- **External APIs**: Call web services

## Tool Lifecycle

```
User Query → LLM → Tool Call Request → Tool Execution → Result → LLM → Final Response
```

## Implementing the Tool Trait

```rust
use mojentic::llm::Tool;
use serde_json::{json, Value};
use async_trait::async_trait;

pub struct Calculator;

#[async_trait]
impl Tool for Calculator {
    fn name(&self) -> &str {
        "calculator"
    }
    
    fn description(&self) -> &str {
        "Perform basic arithmetic operations"
    }
    
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["add", "subtract", "multiply", "divide"]
                },
                "a": {"type": "number"},
                "b": {"type": "number"}
            },
            "required": ["operation", "a", "b"]
        })
    }
    
    async fn run(&self, arguments: Value) -> Result<Value, Box<dyn std::error::Error>> {
        let operation = arguments["operation"].as_str()
            .ok_or("Missing operation")?;
        let a = arguments["a"].as_f64().ok_or("Invalid number a")?;
        let b = arguments["b"].as_f64().ok_or("Invalid number b")?;
        
        let result = match operation {
            "add" => a + b,
            "subtract" => a - b,
            "multiply" => a * b,
            "divide" if b != 0.0 => a / b,
            "divide" => return Err("Division by zero".into()),
            _ => return Err("Unknown operation".into()),
        };
        
        Ok(json!({"result": result}))
    }
}
```

## Using Tools

```rust
use mojentic::llm::{Broker, Message, Tool};

let messages = vec![Message::user("What is 42 times 17?")];

let tools: Vec<Box<dyn Tool>> = vec![
    Box::new(Calculator),
];

let response = broker
    .generate_with_tools(&messages, &tools, None)
    .await?;

println!("{}", response);  // "714"
```

## Built-in Tools

### DateResolver

Resolves relative dates to ISO 8601:

```rust
use mojentic::llm::tools::DateResolver;

let messages = vec![Message::user("What's the date next Friday?")];
let tools: Vec<Box<dyn Tool>> = vec![Box::new(DateResolver)];

let response = broker
    .generate_with_tools(&messages, &tools, None)
    .await?;
```

Supports:
- "today", "tomorrow", "yesterday"
- "next Monday", "this Friday"
- "in 3 days", "in 1 week"

## Example: Weather Tool

```rust
use mojentic::llm::Tool;
use serde_json::{json, Value};
use async_trait::async_trait;

pub struct WeatherTool {
    api_key: String,
}

#[async_trait]
impl Tool for WeatherTool {
    fn name(&self) -> &str {
        "get_weather"
    }
    
    fn description(&self) -> &str {
        "Get current weather conditions for a location"
    }
    
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "City name or location"
                }
            },
            "required": ["location"]
        })
    }
    
    async fn run(&self, arguments: Value) -> Result<Value, Box<dyn std::error::Error>> {
        let location = arguments["location"]
            .as_str()
            .ok_or("Missing location")?;
        
        // Call weather API
        let weather = self.fetch_weather(location).await?;
        
        Ok(json!({
            "location": location,
            "temperature": weather.temp,
            "condition": weather.condition,
            "humidity": weather.humidity
        }))
    }
}

impl WeatherTool {
    async fn fetch_weather(&self, location: &str) -> Result<Weather, Box<dyn std::error::Error>> {
        // Implementation
        todo!()
    }
}
```

## Error Handling

Tools should return descriptive errors:

```rust
async fn run(&self, arguments: Value) -> Result<Value, Box<dyn std::error::Error>> {
    let param = arguments["required_field"]
        .as_str()
        .ok_or("Missing required_field parameter")?;
    
    // Validate
    if param.is_empty() {
        return Err("required_field cannot be empty".into());
    }
    
    // Process with error handling
    let result = self.risky_operation(param).await?;
    
    Ok(json!({"result": result}))
}
```

## Testing Tools

Tools are easy to unit test:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_calculator_add() {
        let calculator = Calculator;
        let args = json!({
            "operation": "add",
            "a": 5.0,
            "b": 3.0
        });
        
        let result = calculator.run(args).await.unwrap();
        assert_eq!(result["result"], 8.0);
    }
    
    #[tokio::test]
    async fn test_calculator_divide_by_zero() {
        let calculator = Calculator;
        let args = json!({
            "operation": "divide",
            "a": 10.0,
            "b": 0.0
        });
        
        assert!(calculator.run(args).await.is_err());
    }
}
```

## Best Practices

### 1. Keep Tools Focused

One tool, one purpose:

```rust
// Good: Specific purpose
pub struct GetWeather;
pub struct GetForecast;

// Avoid: Too broad
pub struct WeatherOperations;
```

### 2. Validate Inputs

```rust
async fn run(&self, arguments: Value) -> Result<Value, Box<dyn std::error::Error>> {
    let email = arguments["email"]
        .as_str()
        .ok_or("Missing email")?;
    
    if !validate_email(email) {
        return Err("Invalid email format".into());
    }
    
    // Proceed with valid email
    Ok(json!({"sent": true}))
}
```

### 3. Provide Clear Errors

```rust
Err(format!("User not found: {}", user_id).into())
Err("Invalid date format. Use YYYY-MM-DD".into())
Err("Rate limit exceeded. Try again in 60 seconds".into())
```

### 4. Return Structured Data

```rust
Ok(json!({
    "success": true,
    "data": {
        "user": {
            "name": "Alice",
            "age": 30
        }
    },
    "timestamp": chrono::Utc::now()
}))
```

### 5. Use Async for I/O

```rust
#[async_trait]
impl Tool for DatabaseQuery {
    async fn run(&self, arguments: Value) -> Result<Value, Box<dyn std::error::Error>> {
        // Async database query
        let results = sqlx::query("SELECT * FROM users")
            .fetch_all(&self.pool)
            .await?;
        
        Ok(json!({"results": results}))
    }
}
```

## Multiple Tools

LLMs can use multiple tools:

```rust
let tools: Vec<Box<dyn Tool>> = vec![
    Box::new(DateResolver),
    Box::new(Calculator),
    Box::new(WeatherTool::new(api_key)),
    Box::new(DatabaseQuery::new(pool)),
];

let messages = vec![Message::user("complex query")];

let response = broker
    .generate_with_tools(&messages, &tools, None)
    .await?;
```

## See Also

- [Getting Started](./getting-started.md)
- [Broker Guide](./broker.md)
- [API: Tools](./api-tools.md)
