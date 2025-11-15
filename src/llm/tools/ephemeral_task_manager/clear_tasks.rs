use super::task_list::TaskList;
use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Tool for clearing all tasks from the ephemeral task manager
pub struct ClearTasksTool {
    task_list: Arc<Mutex<TaskList>>,
}

impl ClearTasksTool {
    /// Creates a new ClearTasksTool with a shared task list
    pub fn new(task_list: Arc<Mutex<TaskList>>) -> Self {
        Self { task_list }
    }
}

impl LlmTool for ClearTasksTool {
    fn run(&self, _args: &HashMap<String, Value>) -> Result<Value> {
        let mut task_list = self.task_list.lock().unwrap();
        let count = task_list.clear_tasks();

        Ok(json!({
            "count": count,
            "summary": format!("Cleared {} tasks from the list", count)
        }))
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            r#type: "function".to_string(),
            function: FunctionDescriptor {
                name: "clear_tasks".to_string(),
                description: "Remove all tasks from the task list.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {}
                }),
            },
        }
    }
}
