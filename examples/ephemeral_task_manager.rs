use mojentic::llm::broker::LlmBroker;
use mojentic::llm::gateways::ollama::OllamaGateway;
use mojentic::llm::LlmMessage;
use mojentic::llm::tools::ephemeral_task_manager::{all_tools, TaskList};
use std::sync::{Arc, Mutex};

/// Example demonstrating the usage of the ephemeral task manager tools.
///
/// Run with: cargo run --example ephemeral_task_manager
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create broker with Ollama
    let gateway = OllamaGateway::new();
    let broker = LlmBroker::new("qwen3:32b".to_string(), Arc::new(gateway));

    // Create shared task list
    let task_list = Arc::new(Mutex::new(TaskList::new()));

    // Create all task management tools
    let tools = all_tools(Arc::clone(&task_list));

    // Ask the LLM to manage a counting task
    let message = LlmMessage::user(
        "I want you to count from 1 to 5. Break that request down into individual tasks, \
         track them using available tools, and perform them one by one until you're finished. \
         Report on your progress as you work through the tasks."
            .to_string(),
    );

    println!("Starting task management example...");
    println!("{}", "=".repeat(80));
    println!();

    // Generate response with tools
    match broker
        .generate(&[message], Some(&tools), None)
        .await
    {
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
    println!("Final Task List:");
    println!();

    if tasks.is_empty() {
        println!("No tasks in list");
    } else {
        for task in tasks {
            println!("{}. {} ({})", task.id, task.description, task.status.as_str());
        }
    }

    Ok(())
}
