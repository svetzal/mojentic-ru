use super::task_list::TaskList;
use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Tool for inserting a new task after a specific task ID in the ephemeral task manager list
#[derive(Clone)]
pub struct InsertTaskAfterTool {
    task_list: Arc<Mutex<TaskList>>,
}

impl InsertTaskAfterTool {
    /// Creates a new InsertTaskAfterTool with a shared task list
    pub fn new(task_list: Arc<Mutex<TaskList>>) -> Self {
        Self { task_list }
    }
}

impl LlmTool for InsertTaskAfterTool {
    fn run(&self, args: &HashMap<String, Value>) -> Result<Value> {
        let existing_task_id =
            args.get("existing_task_id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        let description =
            args.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let mut task_list = self.task_list.lock().unwrap();
        let task = task_list.insert_task_after(existing_task_id, description)?;

        Ok(json!({
            "id": task.id,
            "description": task.description,
            "status": task.status.as_str(),
            "summary": format!("Task '{}' inserted after task '{}' successfully", task.id, existing_task_id)
        }))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "insert_task_after".to_string(),
                description: "Insert a new task after an existing task in the task list. The task will start with 'pending' status.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "existing_task_id": {
                            "type": "integer",
                            "description": "The ID of the existing task after which to insert the new task"
                        },
                        "description": {
                            "type": "string",
                            "description": "The description of the new task"
                        }
                    },
                    "required": ["existing_task_id", "description"]
                }),
            },
        }
    }
    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(self.clone())
    }
}
