use super::task_list::TaskList;
use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Tool for prepending a new task to the beginning of the ephemeral task manager list
pub struct PrependTaskTool {
    task_list: Arc<Mutex<TaskList>>,
}

impl PrependTaskTool {
    /// Creates a new PrependTaskTool with a shared task list
    pub fn new(task_list: Arc<Mutex<TaskList>>) -> Self {
        Self { task_list }
    }
}

impl LlmTool for PrependTaskTool {
    fn run(&self, args: &HashMap<String, Value>) -> Result<Value> {
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut task_list = self.task_list.lock().unwrap();
        let task = task_list.prepend_task(description);

        Ok(json!({
            "id": task.id,
            "description": task.description,
            "status": task.status.as_str(),
            "summary": format!("Task '{}' prepended successfully", task.id)
        }))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "prepend_task".to_string(),
                description: "Prepend a new task to the beginning of the task list with a description. The task will start with 'pending' status.".to_string(),
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
