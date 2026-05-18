use super::task_list::TaskList;
use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Tool for prepending a new task to the beginning of the ephemeral task manager list
#[derive(Clone)]
pub struct PrependTaskTool {
    task_list: Arc<Mutex<TaskList>>,
}

impl PrependTaskTool {
    /// Creates a new PrependTaskTool with a shared task list
    pub fn new(task_list: Arc<Mutex<TaskList>>) -> Self {
        Self { task_list }
    }
}

#[async_trait]
impl LlmTool for PrependTaskTool {
    async fn run(
        &self,
        args: &HashMap<String, Value>,
        _ctx: &crate::llm::tools::ToolRunCtx,
    ) -> Result<Value> {
        let description =
            args.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();

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
    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::tools::ToolRunCtx;

    #[tokio::test]
    async fn test_prepend_task_run() {
        let task_list = Arc::new(Mutex::new(TaskList::new()));
        let tool = PrependTaskTool::new(Arc::clone(&task_list));
        let mut args = HashMap::new();
        args.insert("description".to_string(), serde_json::Value::from("first task"));
        let result = tool.run(&args, &ToolRunCtx::default()).await.unwrap();
        assert_eq!(result["description"], "first task");
        assert_eq!(result["status"], "pending");
    }

    #[test]
    fn test_descriptor() {
        let task_list = Arc::new(Mutex::new(TaskList::new()));
        let tool = PrependTaskTool::new(task_list);
        assert_eq!(tool.descriptor().function.name, "prepend_task");
    }
}
