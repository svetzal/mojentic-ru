# Simple Recursive Agent

The `SimpleRecursiveAgent` provides an event-driven approach to iterative problem-solving with LLMs. It automatically handles retries, tool execution, and state management while emitting events at each step for monitoring.

## Overview

The SimpleRecursiveAgent:
- Solves problems through iterative refinement
- Emits events at each step for monitoring and debugging
- Handles tool execution automatically via ChatSession
- Stops when it finds a solution, fails, or reaches max iterations
- Provides timeout protection (300 seconds default)

## Basic Usage

```rust
use mojentic::agents::SimpleRecursiveAgent;
use mojentic::llm::{LlmBroker, LlmMessage};
use mojentic::llm::gateways::OllamaGateway;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gateway = OllamaGateway::default();
    let broker = Arc::new(LlmBroker::new("qwen3:32b", gateway));

    // Create agent with 5 max iterations
    let agent = SimpleRecursiveAgent::new(broker, Vec::new(), Some(5), None);

    // Solve a problem
    let solution = agent.solve("What is the capital of France?").await?;
    println!("{}", solution);

    Ok(())
}
```

## With Tools

The agent can use tools to gather information or perform actions:

```rust
use mojentic::llm::tools::simple_date_tool::SimpleDateTool;
use std::sync::Arc;

let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(SimpleDateTool)];

let agent = SimpleRecursiveAgent::new(
    broker,
    tools,
    Some(5),  // Max iterations
    None      // Use default system prompt
);

let solution = agent.solve("What's the date next Friday?").await?;
```

## Event Monitoring

Subscribe to events to monitor the problem-solving process:

```rust
use mojentic::agents::{SimpleRecursiveAgent, RecursiveAgentEvent};

let agent = SimpleRecursiveAgent::new(broker, vec![], Some(5), None);

// Subscribe to events before solving
let subscription = agent.subscribe({
    let agent_clone = agent.clone();
    move |event| {
        match event {
            RecursiveAgentEvent::GoalSubmitted { goal } => {
                println!("Goal submitted: {}", goal);
            }
            RecursiveAgentEvent::IterationCompleted { iteration, response, .. } => {
                println!("Iteration {}: {}", iteration, response);
            }
            RecursiveAgentEvent::GoalAchieved { solution, iterations } => {
                println!("Success after {} iterations!", iterations);
            }
            RecursiveAgentEvent::GoalFailed { reason } => {
                eprintln!("Failed: {}", reason);
            }
            RecursiveAgentEvent::Timeout => {
                eprintln!("Timeout after 300 seconds");
            }
        }
    }
});

let solution = agent.solve("Complex problem").await?;

// Unsubscribe when done
agent.unsubscribe(subscription);
```

## Custom System Prompt

Customize the agent's behavior with a custom system prompt:

```rust
let custom_prompt =
    "You are a concise assistant that provides brief, factual answers. \
     Always respond in exactly one sentence.";

let agent = SimpleRecursiveAgent::new(
    broker,
    vec![],
    Some(5),
    Some(custom_prompt.to_string())
);
```

## Goal State

The state object that tracks the problem-solving process:

```rust
pub struct GoalState {
    pub goal: String,
    pub iteration: usize,
    pub max_iterations: usize,
    pub solution: Option<String>,
    pub is_complete: bool,
}
```

## Event Types

```rust
pub enum RecursiveAgentEvent {
    GoalSubmitted { goal: String },
    IterationCompleted {
        iteration: usize,
        response: String,
        state: GoalState,
    },
    GoalAchieved {
        solution: String,
        iterations: usize,
    },
    GoalFailed { reason: String },
    Timeout,
}
```

## Completion Criteria

The agent stops iterating when:

1. **Success**: The LLM response contains "DONE" (case-insensitive)
2. **Failure**: The LLM response contains "FAIL" (case-insensitive)
3. **Max Iterations**: The iteration count reaches `max_iterations`
4. **Timeout**: 300 seconds have elapsed

When stopped at max iterations, the last response is returned as the best available solution.

## API Reference

### Constructor

```rust
impl SimpleRecursiveAgent {
    pub fn new(
        llm: Arc<LlmBroker>,
        available_tools: Vec<Box<dyn LlmTool>>,
        max_iterations: Option<usize>,
        system_prompt: Option<String>,
    ) -> Self
}
```

**Parameters:**
- `llm`: The LLM broker to use for generating responses
- `available_tools`: Vector of tools the agent can use
- `max_iterations`: Maximum number of iterations (default: 5)
- `system_prompt`: Custom system prompt (default: problem-solving assistant prompt)

### Methods

#### solve(&self, problem: &str) -> Result<String>

Solve a problem asynchronously.

**Parameters:**
- `problem`: The problem to solve

**Returns:** `Result<String>` containing the solution

**Errors:** Returns error if the solution cannot be found within 300 seconds

#### subscribe<F>(&self, callback: F) -> usize

Subscribe to agent events.

**Parameters:**
- `callback`: Function to call when events occur

**Returns:** Subscription ID for unsubscribing

#### unsubscribe(&self, id: usize)

Unsubscribe from events using subscription ID.

## Best Practices

1. **Use Arc for sharing**: The agent uses `Arc<LlmBroker>` for thread-safe sharing:
   ```rust
   let broker = Arc::new(LlmBroker::new("model", gateway));
   let agent = SimpleRecursiveAgent::new(broker, vec![], Some(5), None);
   ```

2. **Set appropriate max iterations**: Balance between thoroughness and performance:
   - Simple queries: 3-5 iterations
   - Complex problems: 10-20 iterations

3. **Use event monitoring for debugging**: Subscribe to events during development to understand the agent's reasoning process

4. **Clean up subscriptions**: Always unsubscribe when done to prevent memory leaks

5. **Provide clear problem statements**: The more specific your problem description, the better the agent can solve it

## Example: Complete Workflow

```rust
use mojentic::agents::{SimpleRecursiveAgent, RecursiveAgentEvent};
use mojentic::llm::{LlmBroker, LlmMessage};
use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::tools::simple_date_tool::SimpleDateTool;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gateway = OllamaGateway::default();
    let broker = Arc::new(LlmBroker::new("qwen3:32b", gateway));

    let tools: Vec<Box<dyn LlmTool>> = vec![Box::new(SimpleDateTool)];

    let agent = SimpleRecursiveAgent::new(broker, tools, Some(5), None);

    // Log progress
    let sub = agent.subscribe(|event| {
        match event {
            RecursiveAgentEvent::IterationCompleted { iteration, .. } => {
                println!("Iteration {}/5", iteration);
            }
            RecursiveAgentEvent::GoalAchieved { iterations, .. } => {
                println!("✓ Solved in {} iterations", iterations);
            }
            _ => {}
        }
    });

    let solution = agent
        .solve("What's the date two Fridays from now?")
        .await?;

    println!("\nSolution: {}", solution);

    agent.unsubscribe(sub);

    Ok(())
}
```

## Concurrent Problem Solving

The agent is thread-safe and can solve multiple problems concurrently:

```rust
use tokio::task;

let agent = Arc::new(SimpleRecursiveAgent::new(broker, vec![], Some(3), None));

let problems = vec![
    "What is the Pythagorean theorem?",
    "Explain recursion in programming.",
];

let handles: Vec<_> = problems
    .into_iter()
    .map(|problem| {
        let agent = Arc::clone(&agent);
        task::spawn(async move { agent.solve(problem).await })
    })
    .collect();

for (i, handle) in handles.into_iter().enumerate() {
    let solution = handle.await??;
    println!("Problem {}: {}", i + 1, solution);
}
```

## Comparison with IterativeProblemSolver

Both agents solve problems iteratively, but they differ in approach:

**SimpleRecursiveAgent:**
- Event-driven architecture
- Explicit event types for each stage
- Manual event subscription for monitoring
- 300-second hard timeout
- Best for: Custom event handling, complex workflows, debugging

**IterativeProblemSolver:**
- Simpler API, minimal boilerplate
- Direct access to chat history
- Best for: Quick prototyping, straightforward tasks

Choose `SimpleRecursiveAgent` when you need fine-grained control and visibility into the problem-solving process. Choose `IterativeProblemSolver` for simpler use cases.
