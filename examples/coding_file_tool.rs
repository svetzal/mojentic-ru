/// Example demonstrating coding with file management tools and task tracking.
///
/// ⚠️  **CURRENTLY DISABLED**: The file_manager module needs trait migration.
///
/// This example demonstrates what the coding workflow would look like once
/// file_manager is re-enabled. It would combine:
/// - File management tools (read, write, list, find, create directories)
/// - Task management tools (for planning and tracking work)
/// - LLM-driven coding assistance
///
/// ## What needs to be fixed in file_manager.rs:
///
/// The file_manager.rs module was written for an older `Tool` trait that no longer exists.
/// It needs to be migrated to the new `LlmTool` trait. Here are the required changes:
///
/// ### 1. Update imports (line ~6):
/// ```rust
/// // OLD:
/// use crate::llm::tools::Tool;
///
/// // NEW:
/// use crate::llm::tools::{LlmTool, ToolDescriptor, FunctionDescriptor};
/// use std::collections::HashMap;
/// ```
///
/// ### 2. Update trait implementations:
/// ```rust
/// // OLD:
/// impl Tool for SomeTool { ... }
///
/// // NEW:
/// impl LlmTool for SomeTool { ... }
/// ```
///
/// ### 3. Update descriptor() method signature and return type:
/// ```rust
/// // OLD:
/// fn descriptor(&self) -> Value {
///     json!({ "type": "function", "function": { ... } })
/// }
///
/// // NEW:
/// fn descriptor(&self) -> ToolDescriptor {
///     ToolDescriptor {
///         r#type: "function".to_string(),
///         function: FunctionDescriptor {
///             name: "tool_name".to_string(),
///             description: "description".to_string(),
///             parameters: json!({ /* schema */ }),
///         },
///     }
/// }
/// ```
///
/// ### 4. Update run() method signature:
/// ```rust
/// // OLD:
/// fn run(&self, args: Value) -> Result<String> {
///     let param = args["param"].as_str().ok_or(...)?;
///     // ...
///     Ok("result string")
/// }
///
/// // NEW:
/// fn run(&self, args: &HashMap<String, Value>) -> Result<Value> {
///     let param = args.get("param")
///         .and_then(|v| v.as_str())
///         .ok_or_else(|| MojenticError::ToolError("Missing 'param'".to_string()))?;
///     // ...
///     Ok(json!("result string"))  // Wrap results in json!()
/// }
/// ```
///
/// ### 5. Fix error construction:
/// ```rust
/// // OLD:
/// MojenticError::Tool {
///     message: format!("error message"),
///     source: None,
/// }
///
/// // NEW:
/// MojenticError::ToolError(format!("error message"))
/// ```
///
/// ### Reference Implementation:
/// See `src/llm/tools/simple_date_tool.rs` for a complete working example
/// of the LlmTool trait pattern.
///
/// ## Steps to complete:
///
/// 1. Apply the changes above to `src/llm/tools/file_manager.rs`
/// 2. Uncomment `pub mod file_manager;` in `src/llm/tools/mod.rs`
/// 3. Run: `cargo build --all-features`
/// 4. Fix any remaining compilation errors
/// 5. Run: `cargo test`
/// 6. Update this example to use the working file tools
///
/// ## Once fixed, this example will:
///
/// - Create a sandbox directory
/// - Initialize FilesystemGateway and all file tools
/// - Combine with task management for systematic coding
/// - Ask LLM to create a simple Rust calculator module with tests
/// - Track progress using the ephemeral task manager
///
/// Run with: `cargo run --example coding_file_tool`

use mojentic::llm::broker::LlmBroker;
use mojentic::llm::gateways::ollama::OllamaGateway;
use mojentic::llm::tools::ephemeral_task_manager::{all_tools, TaskList};
use mojentic::llm::LlmMessage;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "=".repeat(80));
    println!("Coding File Tool Example (PLACEHOLDER)");
    println!("{}", "=".repeat(80));
    println!();

    println!("⚠️  NOTE: The file_manager module needs trait migration.");
    println!();
    println!("This example is a placeholder demonstrating the intended workflow.");
    println!("See the module documentation above for detailed fix instructions.");
    println!();

    // Create a temporary directory to show what would be created
    let sandbox_dir = TempDir::new()?;
    let sandbox_path = sandbox_dir.path();

    println!("Sandbox directory would be: {:?}", sandbox_path);
    println!();

    // Demonstrate task management (which IS working)
    println!("Demonstrating task management (which works):");
    println!("{}", "-".repeat(80));
    println!();

    let gateway = OllamaGateway::new();
    let broker = LlmBroker::new("qwen3-coder:30b".to_string(), Arc::new(gateway));

    let task_list = Arc::new(Mutex::new(TaskList::new()));
    let tools = all_tools(Arc::clone(&task_list));

    let message = LlmMessage::user(
        "Create a plan for building a simple Rust calculator module. \
         Break it into tasks: create module structure, implement operations, \
         add tests, write documentation. Use the task tools to track this plan."
            .to_string(),
    );

    println!("Asking LLM to create a project plan...");
    println!();

    match broker.generate(&[message], Some(&tools), None).await {
        Ok(response) => {
            println!("LLM Response:");
            println!("{}", response);
            println!();
        }
        Err(e) => {
            eprintln!("Error: {:?}", e);
        }
    }

    // Show final task list
    let tasks = task_list.lock().unwrap().list_tasks();

    println!();
    println!("{}", "=".repeat(80));
    println!("Task List Created:");
    println!("{}", "=".repeat(80));
    println!();

    if tasks.is_empty() {
        println!("No tasks in list");
    } else {
        for task in tasks {
            println!("{}. {} ({})", task.id, task.description, task.status.as_str());
        }
    }

    println!();
    println!("Once file_manager is fixed, the LLM would execute these tasks");
    println!("using file tools to actually create the calculator module.");
    println!();

    println!("📖 See module documentation for detailed fix instructions.");
    println!();

    Ok(())
}

