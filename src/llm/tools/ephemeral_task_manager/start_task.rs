use super::task_list::TaskList;
use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Tool for starting a task in the ephemeral task manager
///
/// This tool changes a task's status from Pending to InProgress
pub struct StartTaskTool {
    task_list: Arc<Mutex<TaskList>>,
}

impl StartTaskTool {
    /// Creates a new StartTaskTool with a shared task list
    pub fn new(task_list: Arc<Mutex<TaskList>>) -> Self {
        Self { task_list }
    }
}

impl LlmTool for StartTaskTool {
    fn run(&self, args: &HashMap<String, Value>) -> Result<Value> {
        let task_id = args.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        let mut task_list = self.task_list.lock().unwrap();
        let task = task_list.start_task(task_id)?;

        Ok(json!({
            "id": task.id,
            "description": task.description,
            "status": task.status.as_str(),
            "summary": format!("Task '{}' started successfully", task_id)
        }))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "start_task".to_string(),
                description: "Start a task by changing its status from PENDING to IN_PROGRESS."
                    .to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "integer",
                            "description": "The ID of the task to start"
                        }
                    },
                    "required": ["id"]
                }),
            },
        }
    }
}
