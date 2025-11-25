use super::task_list::TaskList;
use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Tool for listing all tasks in the ephemeral task manager
#[derive(Clone)]
pub struct ListTasksTool {
    task_list: Arc<Mutex<TaskList>>,
}

impl ListTasksTool {
    /// Creates a new ListTasksTool with a shared task list
    pub fn new(task_list: Arc<Mutex<TaskList>>) -> Self {
        Self { task_list }
    }

    /// Formats a list of tasks as a string
    fn format_tasks(&self, tasks: &[super::task::Task]) -> String {
        if tasks.is_empty() {
            return "No tasks found.".to_string();
        }

        tasks
            .iter()
            .map(|task| format!("{}. {} ({})", task.id, task.description, task.status.as_str()))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl LlmTool for ListTasksTool {
    fn run(&self, _args: &HashMap<String, Value>) -> Result<Value> {
        let task_list = self.task_list.lock().unwrap();
        let tasks = task_list.list_tasks();
        let task_list_str = self.format_tasks(&tasks);

        Ok(json!({
            "count": tasks.len(),
            "tasks": task_list_str,
            "summary": format!("Found {} tasks\n\n{}", tasks.len(), task_list_str)
        }))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "list_tasks".to_string(),
                description: "List all tasks in the task list.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
        }
    }
    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(self.clone())
    }
}
