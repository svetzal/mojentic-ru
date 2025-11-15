use super::task_list::TaskList;
use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Tool for appending a new task to the end of the ephemeral task manager list
pub struct AppendTaskTool {
    task_list: Arc<Mutex<TaskList>>,
}

impl AppendTaskTool {
    /// Creates a new AppendTaskTool with a shared task list
    pub fn new(task_list: Arc<Mutex<TaskList>>) -> Self {
        Self { task_list }
    }
}

impl LlmTool for AppendTaskTool {
    fn run(&self, args: &HashMap<String, Value>) -> Result<Value> {
        let description =
            args.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let mut task_list = self.task_list.lock().unwrap();
        let task = task_list.append_task(description);

        Ok(json!({
            "id": task.id,
            "description": task.description,
            "status": task.status.as_str(),
            "summary": format!("Task '{}' appended successfully", task.id)
        }))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "append_task".to_string(),
                description: "Append a new task to the end of the task list with a description. The task will start with 'pending' status.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "description": {
                            "type": "string",
                            "description": "The description of the task"
                        }
                    },
                    "required": ["description"]
                }),
            },
        }
    }
}
