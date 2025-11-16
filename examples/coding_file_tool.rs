/// Example demonstrating LLM-driven coding with file management and task tracking.
///
/// This example combines:
/// - File management tools (read, write, list, find, create directories)
/// - Task management tools (for planning and tracking work)
/// - LLM-driven coding assistance via Ollama
///
/// The example creates a sandboxed environment and asks the LLM to build a simple
/// Rust calculator module, tracking progress using the ephemeral task manager.
///
/// Run with: `cargo run --example coding_file_tool`
use mojentic::llm::broker::LlmBroker;
use mojentic::llm::gateways::ollama::OllamaGateway;
use mojentic::llm::tools::ephemeral_task_manager::{all_tools as task_tools, TaskList};
use mojentic::llm::tools::file_manager::{
    CreateDirectoryTool, FilesystemGateway, ListAllFilesTool, ReadFileTool, WriteFileTool,
};
use mojentic::llm::tools::LlmTool;
use mojentic::llm::LlmMessage;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "=".repeat(80));
    println!("🚀 Mojentic Rust - LLM-Driven Coding Example");
    println!("{}", "=".repeat(80));
    println!();

    // Create a sandboxed temporary directory
    let sandbox_dir = TempDir::new()?;
    let sandbox_path = sandbox_dir.path();

    println!("📁 Sandbox directory: {:?}", sandbox_path);
    println!();

    // Initialize the filesystem gateway and file tools
    let fs_gateway = FilesystemGateway::new(sandbox_path)?;

    let create_dir_tool = CreateDirectoryTool::new(fs_gateway.clone());
    let write_file_tool = WriteFileTool::new(fs_gateway.clone());
    let read_file_tool = ReadFileTool::new(fs_gateway.clone());
    let list_all_tool = ListAllFilesTool::new(fs_gateway.clone());

    // Initialize task manager
    let task_list = Arc::new(Mutex::new(TaskList::new()));
    let task_mgmt_tools = task_tools(Arc::clone(&task_list));

    // Combine all tools for the LLM
    let mut tools: Vec<Box<dyn LlmTool>> = vec![
        Box::new(create_dir_tool),
        Box::new(write_file_tool),
        Box::new(read_file_tool),
        Box::new(list_all_tool),
    ];
    tools.extend(task_mgmt_tools);

    // Initialize LLM broker with Ollama
    let gateway = OllamaGateway::new();
    let broker = LlmBroker::new("qwen3-coder:30b".to_string(), Arc::new(gateway), None);

    println!("🤖 Initializing LLM (qwen3-coder:30b via Ollama)...");
    println!();

    // System prompt to guide the LLM's behavior
    let system_message = LlmMessage::system(
        "You are an expert Rust developer. You have access to file management tools \
         and task tracking tools. When given a coding task:\n\
         1. Break it down into concrete steps using task management tools\n\
         2. Execute each step systematically using file tools\n\
         3. Mark tasks as complete as you finish them\n\
         4. Create clean, well-documented, idiomatic Rust code\n\
         5. Include tests for the code you create\n\n\
         Work methodically through the task list until all tasks are complete."
            .to_string(),
    );

    // User request to build a calculator module
    let user_message = LlmMessage::user(
        "Create a simple Rust calculator module in this sandbox. \
         The module should:\n\
         1. Be in a 'calculator' directory with proper structure\n\
         2. Have a lib.rs file with a Calculator struct\n\
         3. Implement basic operations: add, subtract, multiply, divide\n\
         4. Include proper error handling for division by zero\n\
         5. Have a tests.rs file with unit tests for all operations\n\
         6. Have a README.md explaining the module\n\n\
         Use the task management tools to plan your work, then execute the plan \
         using the file management tools. Show your progress as you work."
            .to_string(),
    );

    let mut messages = vec![system_message, user_message];

    println!("📋 Requesting LLM to build calculator module...");
    println!("{}", "-".repeat(80));
    println!();

    // Iterative conversation loop - allow LLM to use tools multiple times
    let max_iterations = 15;
    let mut iteration = 0;

    while iteration < max_iterations {
        iteration += 1;

        println!("🔄 Iteration {}/{}", iteration, max_iterations);
        println!();

        match broker.generate(&messages, Some(&tools), None, None).await {
            Ok(response) => {
                println!("💬 LLM Response:");
                println!("{}", response);
                println!();

                // Add assistant's response to conversation history
                messages.push(LlmMessage::assistant(response.clone()));

                // Check if the LLM indicates completion
                let response_lower = response.to_lowercase();
                if response_lower.contains("all tasks complete")
                    || response_lower.contains("finished")
                    || response_lower.contains("done")
                {
                    // Verify tasks are actually complete
                    let tasks = task_list.lock().unwrap().list_tasks();
                    let all_complete = tasks
                        .iter()
                        .all(|t| t.status.as_str() == "completed" || t.status.as_str() == "done");

                    if all_complete && !tasks.is_empty() {
                        println!("✅ LLM has completed all tasks!");
                        break;
                    }
                }

                // If response looks like final output without tool calls, we might be done
                if !response.contains("tool_calls") && iteration > 3 {
                    println!("ℹ️  No more tool calls detected, checking task completion...");
                    let tasks = task_list.lock().unwrap().list_tasks();
                    if !tasks.is_empty() {
                        let all_complete = tasks.iter().all(|t| {
                            t.status.as_str() == "completed" || t.status.as_str() == "done"
                        });
                        if all_complete {
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("❌ Error: {:?}", e);
                break;
            }
        }

        println!("{}", "-".repeat(80));
        println!();
    }

    if iteration >= max_iterations {
        println!("⚠️  Reached maximum iterations ({})", max_iterations);
    }

    // Display final task list
    println!();
    println!("{}", "=".repeat(80));
    println!("📊 Final Task Status:");
    println!("{}", "=".repeat(80));
    println!();

    let tasks = task_list.lock().unwrap().list_tasks();

    if tasks.is_empty() {
        println!("No tasks in list");
    } else {
        for task in tasks {
            let status_emoji = match task.status.as_str() {
                "completed" | "done" => "✅",
                "in_progress" | "started" => "🔄",
                _ => "⏸️",
            };
            println!(
                "{} {}. {} ({})",
                status_emoji,
                task.id,
                task.description,
                task.status.as_str()
            );
        }
    }

    // Display created files
    println!();
    println!("{}", "=".repeat(80));
    println!("📁 Files Created in Sandbox:");
    println!("{}", "=".repeat(80));
    println!();

    // Create a new list tool instance to display final files
    let final_list_tool = ListAllFilesTool::new(fs_gateway.clone());
    let list_result = final_list_tool
        .run(&std::collections::HashMap::from([("path".to_string(), serde_json::json!("."))]))?;

    println!("{}", list_result);
    println!();

    println!("{}", "=".repeat(80));
    println!("✅ Example completed successfully!");
    println!("{}", "=".repeat(80));
    println!();
    println!("This example demonstrated:");
    println!("  ✓ LLM-driven code generation");
    println!("  ✓ Task planning and tracking");
    println!("  ✓ File creation and management");
    println!("  ✓ Sandboxed development environment");
    println!("  ✓ Multi-step workflow execution");
    println!();
    println!("The calculator module was created in: {:?}", sandbox_path);
    println!();

    Ok(())
}
