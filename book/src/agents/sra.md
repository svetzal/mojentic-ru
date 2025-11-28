# Tutorial: Building Autonomous Agents

## What is an Autonomous Agent?

An autonomous agent is a system that can perceive its environment, reason about how to achieve a goal, and take actions (use tools) to accomplish that goal. Unlike a simple chatbot, an agent has a loop of "Thought -> Action -> Observation".

## The Simple Recursive Agent (SRA)

Mojentic provides a `SimpleRecursiveAgent` pattern. This agent:
1.  Receives a goal.
2.  Thinks about the next step.
3.  Selects a tool to use.
4.  Executes the tool.
5.  Observes the result.
6.  Repeats until the goal is met.

## Building an Agent

Let's build an agent that can answer questions using a web search tool.

### 1. Setup

You'll need the `WebSearchTool` (or any other tool) and a Broker.

```rust
use mojentic::prelude::*;
use mojentic::tools::WebSearchTool;
use mojentic::agents::SimpleRecursiveAgent;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize broker
    let gateway = Arc::new(OllamaGateway::new());
    let broker = LlmBroker::new("qwen3:32b", gateway, None);

    // Configure tools
    let tools: Vec<Arc<dyn LlmTool>> = vec![
        Arc::new(WebSearchTool::new(SearchProvider::Tavily)),
    ];
    
    // ...
    Ok(())
}
```

### 2. Run the Agent

```rust
let goal = "Find out who won the latest Super Bowl and tell me the score.";

let result = SimpleRecursiveAgent::run(&broker, goal, tools, None).await?;

println!("Final Answer: {}", result);
```

## Step-by-Step Execution

When you run this, the agent enters a loop:

1.  **Thought**: "I need to search for the latest Super Bowl winner."
2.  **Action**: Calls `WebSearchTool` with query "latest Super Bowl winner score".
3.  **Observation**: Receives search results (e.g., "Kansas City Chiefs defeated San Francisco 49ers 25-22...").
4.  **Thought**: "I have the information. I can now answer the user."
5.  **Final Answer**: "The Kansas City Chiefs won the latest Super Bowl with a score of 25-22."

## Customizing the Agent

You can customize the agent's behavior by:
-   **Adding more tools**: Give it file access, calculation abilities, etc.
-   **System Prompt**: Adjust its personality or constraints.
-   **Max Iterations**: Limit how many steps it can take to prevent infinite loops.

```rust
let config = AgentConfig {
    max_iterations: 10,
    ..Default::default()
};

SimpleRecursiveAgent::run(&broker, goal, tools, Some(config)).await?;
```

## Summary

Autonomous agents allow you to solve complex, multi-step problems. By combining a reasoning loop with a set of tools, you can build systems that can interact with the world to achieve user goals.
