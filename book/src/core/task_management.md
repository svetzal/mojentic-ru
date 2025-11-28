# Example: Task Management

The `TaskManager` is an example of how to build stateful tools that allow agents to manage ephemeral tasks. This reference implementation shows how to maintain state across tool calls.

## Features

- **Create Tasks**: Add new tasks to the list
- **List Tasks**: View all current tasks and their status
- **Complete Tasks**: Mark tasks as done
- **Prioritize**: Agents can determine the order of execution

## Usage

```rust
use mojentic::prelude::*;
use mojentic::tools::TaskManager;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize broker
    let gateway = Arc::new(OllamaGateway::new());
    let broker = LlmBroker::new("qwen3:32b", gateway, None);

    // Register the tool
    let tools: Vec<Arc<dyn LlmTool>> = vec![
        Arc::new(TaskManager::new()),
    ];

    // The agent can now manage its own tasks
    let messages = vec![
        LlmMessage::system("You are a helpful assistant. Use the task manager to track your work."),
        LlmMessage::user("Plan a party for 10 people."),
    ];

    let response = broker.generate(&messages, Some(tools), None).await?;
    println!("{}", response);
    
    Ok(())
}
```

## Integration with Agents

The Task Manager is particularly powerful when combined with the `IterativeProblemSolver` agent, allowing it to maintain state across multiple reasoning steps.
