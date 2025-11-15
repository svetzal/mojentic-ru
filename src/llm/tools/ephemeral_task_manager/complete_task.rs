use super::task_list::TaskList;
use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Tool for completing a task in the ephemeral task manager
///
/// This tool changes a task's status from InProgress to Completed
pub struct CompleteTaskTool {
    task_list: Arc<Mutex<TaskList>>,
}

impl CompleteTaskTool {
    /// Creates a new CompleteTaskTool with a shared task list
    pub fn new(task_list: Arc<Mutex<TaskList>>) -> Self {
        Self { task_list }
    }
}

impl LlmTool for CompleteTaskTool {
    fn run(&self, args: &HashMap<String, Value>) -> Result<Value> {
        let task_id = args
            .get("id")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let mut task_list = self.task_list.lock().unwrap();
        let task = task_list.complete_task(task_id)?;

        Ok(json!({
            "id": task.id,
            "description": task.description,
            "status": task.status.as_str(),
            "summary": format!("Task '{}' completed successfully", task_id)
        }))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "complete_task".to_string(),
                description: "Complete a task by changing its status from IN_PROGRESS to COMPLETED.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "integer",
                            "description": "The ID of the task to complete"
                        }
                    },
                    "required": ["id"]
                }),
            },
        }
    }
}
