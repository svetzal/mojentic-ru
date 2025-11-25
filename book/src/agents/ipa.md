# Iterative Problem Solver

The `IterativeProblemSolver` is an agent that iteratively attempts to solve problems using available tools. It employs a chat-based approach and continues working until it succeeds, fails explicitly, or reaches the maximum number of iterations.

## Overview

The Iterative Problem Solver follows a simple but powerful pattern:

1. **Plan** - Analyze the problem and identify what needs to be done
2. **Act** - Execute actions using available tools
3. **Observe** - Review the results
4. **Refine** - Adjust the approach based on observations
5. **Terminate** - Stop when the goal is met or the iteration budget is exhausted

## Key Features

- **Tool Integration**: Seamlessly integrates with any `LlmTool` implementations
- **Automatic Termination**: Stops when the LLM responds with "DONE" or "FAIL"
- **Iteration Control**: Configurable maximum iteration count prevents infinite loops
- **Chat-Based Context**: Maintains conversation history for context-aware problem solving
- **Summary Generation**: Provides a clean summary of the final result

## Usage

### Basic Example

```rust
use mojentic::agents::IterativeProblemSolver;
use mojentic::llm::{LlmBroker, LlmTool};
use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::tools::simple_date_tool::SimpleDateTool;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the LLM broker
    let gateway = Arc::new(OllamaGateway::default());
    let broker = LlmBroker::new("qwen3:32b", gateway, None);

    // Define available tools
    let tools: Vec<Box<dyn LlmTool>> = vec![
        Box::new(SimpleDateTool),
    ];

    // Create the solver
    let mut solver = IterativeProblemSolver::builder(broker)
        .tools(tools)
        .max_iterations(5)
        .build();

    // Solve a problem
    let result = solver.solve("What's the date next Friday?").await?;
    println!("Result: {}", result);

    Ok(())
}
```

### Custom System Prompt

You can customize the system prompt to guide the solver's behavior:

```rust
let mut solver = IterativeProblemSolver::builder(broker)
    .tools(tools)
    .max_iterations(10)
    .system_prompt(
        "You are a specialized data analysis assistant. \
         Break down complex queries into clear steps and use tools methodically."
    )
    .build();
```

### With Multiple Tools

The solver works best when given appropriate tools for the problem domain:

```rust
use mojentic::llm::tools::ask_user_tool::AskUserTool;
use mojentic::llm::tools::simple_date_tool::SimpleDateTool;

let tools: Vec<Box<dyn LlmTool>> = vec![
    Box::new(AskUserTool::new()),
    Box::new(SimpleDateTool),
];

let mut solver = IterativeProblemSolver::builder(broker)
    .tools(tools)
    .max_iterations(5)
    .build();
```

## How It Works

### Step-by-Step Process

1. **Initialization**: The solver creates a `ChatSession` with the provided system prompt and tools
2. **Iteration Loop**: For each iteration:
   - Sends the problem description with instructions to use tools
   - Checks the response for "DONE" (success) or "FAIL" (failure)
   - Continues if neither keyword is present and iterations remain
3. **Summary**: After termination, requests a concise summary of the result
4. **Return**: Returns the summary as the final result

### Termination Conditions

The solver terminates when one of these conditions is met:

- **Success**: The LLM's response contains "DONE" (case-insensitive)
- **Failure**: The LLM's response contains "FAIL" (case-insensitive)
- **Exhaustion**: The maximum number of iterations is reached

### Logging

The solver uses the `tracing` crate to log important events:

- `info`: Logged when a task completes successfully or fails
- `warn`: Logged when maximum iterations are reached

## Configuration Options

### Builder Pattern

The `IterativeProblemSolver` uses the builder pattern for configuration:

```rust
IterativeProblemSolver::builder(broker)
    .tools(tools)              // Set available tools
    .max_iterations(10)        // Set max iterations (default: 3)
    .system_prompt("...")      // Set custom system prompt
    .build()
```

### Default Values

- **max_iterations**: 3
- **system_prompt**:
  > "You are a problem-solving assistant that can solve complex problems step by step.
  > You analyze problems, break them down into smaller parts, and solve them systematically.
  > If you cannot solve a problem completely in one step, you make progress and identify what to do next."

## Best Practices

### 1. Choose Appropriate Tools

Select tools that are relevant to your problem domain:

```rust
// For date/time problems
let tools = vec![Box::new(SimpleDateTool)];

// For user interaction
let tools = vec![Box::new(AskUserTool::new())];

// For data analysis
let tools = vec![
    Box::new(CalculatorTool),
    Box::new(DataRetrievalTool),
];
```

### 2. Set Reasonable Iteration Limits

Balance between giving the solver enough attempts and preventing excessive computation:

- Simple queries: 3-5 iterations
- Complex analyses: 10-15 iterations
- Open-ended exploration: 20+ iterations

### 3. Provide Context in Problem Description

The more context you provide, the better the solver can work:

```rust
// Less effective
solver.solve("Analyze the data").await?;

// More effective
solver.solve(
    "Analyze the sales data from Q1 2024. \
     Focus on trends in the technology sector. \
     Provide insights on growth patterns."
).await?;
```

### 4. Monitor Logs

Enable tracing to understand solver behavior:

```rust
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::INFO)
    .init();
```

## Common Patterns

### Retry Logic

For operations that might fail transiently:

```rust
let mut attempts = 0;
let max_attempts = 3;

let result = loop {
    attempts += 1;
    match solver.solve(problem).await {
        Ok(result) if !result.contains("FAIL") => break result,
        Ok(_) if attempts < max_attempts => continue,
        Ok(result) => break result,
        Err(e) => return Err(e),
    }
};
```

### Multi-Stage Problems

For problems that require multiple phases:

```rust
// Phase 1: Data gathering
let mut solver = IterativeProblemSolver::builder(broker.clone())
    .tools(data_tools)
    .max_iterations(5)
    .build();
let data = solver.solve("Gather all relevant data").await?;

// Phase 2: Analysis
let mut solver = IterativeProblemSolver::builder(broker)
    .tools(analysis_tools)
    .max_iterations(10)
    .build();
let analysis = solver.solve(&format!("Analyze: {}", data)).await?;
```

## Examples

See the complete examples at:
- `examples/iterative_solver.rs` - Basic usage with date and user interaction tools
- `examples/solver_chat_session.rs` - Interactive chat session with solver delegation pattern

## Error Handling

The solver returns `Result<String, MojenticError>`:

```rust
match solver.solve(problem).await {
    Ok(result) => println!("Solution: {}", result),
    Err(MojenticError::GatewayError(msg)) => {
        eprintln!("Gateway error: {}", msg);
    }
    Err(MojenticError::ToolError(msg)) => {
        eprintln!("Tool error: {}", msg);
    }
    Err(e) => {
        eprintln!("Unexpected error: {}", e);
    }
}
```

## Advanced: Solver as a Tool

The `IterativeProblemSolver` can be wrapped as a tool and used within a `ChatSession`, enabling powerful delegation patterns where a chat assistant can offload complex problems to a specialized solver agent.

### Creating a Solver Tool

```rust
use mojentic::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use mojentic::agents::IterativeProblemSolver;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

struct IterativeProblemSolverTool {
    broker: Arc<LlmBroker>,
    tools: Vec<Box<dyn LlmTool>>,
}

impl IterativeProblemSolverTool {
    fn new(broker: Arc<LlmBroker>, tools: Vec<Box<dyn LlmTool>>) -> Self {
        Self { broker, tools }
    }
}

impl Clone for IterativeProblemSolverTool {
    fn clone(&self) -> Self {
        Self {
            broker: self.broker.clone(),
            tools: self.tools.iter().map(|t| t.clone_box()).collect(),
        }
    }
}

impl LlmTool for IterativeProblemSolverTool {
    fn run(&self, args: &HashMap<String, Value>) -> mojentic::error::Result<Value> {
        let problem_to_solve = args
            .get("problem_to_solve")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                mojentic::error::MojenticError::ToolError(
                    "Missing required argument: problem_to_solve".to_string(),
                )
            })?;

        let solver_tools: Vec<Box<dyn LlmTool>> =
            self.tools.iter().map(|t| t.clone_box()).collect();

        let runtime = tokio::runtime::Handle::current();
        let broker_clone = (*self.broker).clone();

        let result = runtime.block_on(async move {
            let mut solver = IterativeProblemSolver::builder(broker_clone)
                .tools(solver_tools)
                .max_iterations(5)
                .build();

            solver.solve(problem_to_solve).await
        })?;

        Ok(json!({"solution": result}))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "iterative_problem_solver".to_string(),
                description: "Iteratively solve a complex multi-step problem using available tools.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "problem_to_solve": {
                            "type": "string",
                            "description": "The problem or request to be solved."
                        }
                    },
                    "required": ["problem_to_solve"],
                    "additionalProperties": false
                }),
            },
        }
    }

    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(self.clone())
    }
}
```

### Using the Solver Tool in a Chat Session

```rust
use mojentic::llm::{ChatSession, LlmBroker};
use mojentic::llm::tools::simple_date_tool::SimpleDateTool;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gateway = Arc::new(OllamaGateway::default());
    let broker = Arc::new(LlmBroker::new("qwq", gateway, None));

    // Create the solver tool with SimpleDateTool as the inner tool
    let solver_tools: Vec<Box<dyn LlmTool>> = vec![Box::new(SimpleDateTool)];
    let solver_tool = IterativeProblemSolverTool::new(broker.clone(), solver_tools);

    // Create chat session with the solver tool
    let mut session = ChatSession::builder((*broker).clone())
        .system_prompt(
            "You are a helpful assistant with access to an iterative problem solver. \
             When faced with complex multi-step problems or questions that require \
             reasoning and tool usage, use the iterative_problem_solver tool."
        )
        .tools(vec![Box::new(solver_tool)])
        .build();

    // Interactive loop
    loop {
        let mut query = String::new();
        print!("Query: ");
        io::stdout().flush()?;
        io::stdin().read_line(&mut query)?;

        if query.trim().is_empty() {
            break;
        }

        let response = session.send(query.trim()).await?;
        println!("{}\n", response);
    }

    Ok(())
}
```

### Benefits of This Pattern

1. **Delegation**: The chat assistant can offload complex problems to a specialized solver
2. **Composability**: Mix solver capabilities with other tools in the same session
3. **Context Preservation**: The chat session maintains conversation history
4. **Flexible Interaction**: Users can ask simple questions directly or complex problems that trigger the solver

See the complete example at:
- `examples/solver_chat_session.rs` - Interactive chat session with solver delegation

## Limitations

- **LLM Dependency**: Quality of results depends on the underlying LLM's capabilities
- **Tool Design**: Effectiveness relies on well-designed tools with clear descriptions
- **Token Limits**: Long iterations may hit context window limits
- **Cost**: Multiple LLM calls per problem can increase API costs

## See Also

- [Chat Sessions](../core/chat_sessions.md) - Understanding the underlying chat mechanism
- [Building Tools](../core/building_tools.md) - Creating custom tools for the solver
- [Simple Recursive Agent](sra.md) - Alternative problem-solving pattern
