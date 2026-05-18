use super::task_list::TaskList;
use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Tool for completing a task in the ephemeral task manager
///
/// This tool changes a task's status from InProgress to Completed
#[derive(Clone)]
pub struct CompleteTaskTool {
    task_list: Arc<Mutex<TaskList>>,
}

impl CompleteTaskTool {
    /// Creates a new CompleteTaskTool with a shared task list
    pub fn new(task_list: Arc<Mutex<TaskList>>) -> Self {
        Self { task_list }
    }
}

#[async_trait]
impl LlmTool for CompleteTaskTool {
    async fn run(
        &self,
        args: &HashMap<String, Value>,
        _ctx: &crate::llm::tools::ToolRunCtx,
    ) -> Result<Value> {
        let task_id = args.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

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
                description:
                    "Complete a task by changing its status from IN_PROGRESS to COMPLETED."
                        .to_string(),
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
    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::tools::ToolRunCtx;

    #[tokio::test]
    async fn test_complete_task_run() {
        let task_list = Arc::new(Mutex::new(TaskList::new()));
        {
            let mut list = task_list.lock().unwrap();
            let task = list.append_task("do something".to_string());
            list.start_task(task.id).unwrap();
        }
        let tool = CompleteTaskTool::new(Arc::clone(&task_list));
        let mut args = HashMap::new();
        args.insert("id".to_string(), serde_json::Value::from(1u64));
        let result = tool.run(&args, &ToolRunCtx::default()).await.unwrap();
        assert_eq!(result["status"], "completed");
    }

    #[test]
    fn test_descriptor() {
        let task_list = Arc::new(Mutex::new(TaskList::new()));
        let tool = CompleteTaskTool::new(task_list);
        let desc = tool.descriptor();
        assert_eq!(desc.function.name, "complete_task");
    }
}
