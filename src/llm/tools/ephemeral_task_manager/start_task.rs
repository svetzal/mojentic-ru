use super::task_list::TaskList;
use crate::error::Result;
use crate::llm::tools::{FunctionDescriptor, LlmTool, ToolDescriptor};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Tool for starting a task in the ephemeral task manager
///
/// This tool changes a task's status from Pending to InProgress
#[derive(Clone)]
pub struct StartTaskTool {
    task_list: Arc<Mutex<TaskList>>,
}

impl StartTaskTool {
    /// Creates a new StartTaskTool with a shared task list
    pub fn new(task_list: Arc<Mutex<TaskList>>) -> Self {
        Self { task_list }
    }
}

#[async_trait]
impl LlmTool for StartTaskTool {
    async fn run(
        &self,
        args: &HashMap<String, Value>,
        _ctx: &crate::llm::tools::ToolRunCtx,
    ) -> Result<Value> {
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
    fn clone_box(&self) -> Box<dyn LlmTool> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::tools::ToolRunCtx;

    #[tokio::test]
    async fn test_start_task_run() {
        let task_list = Arc::new(Mutex::new(TaskList::new()));
        {
            let mut list = task_list.lock().unwrap();
            list.append_task("pending task".to_string());
        }
        let tool = StartTaskTool::new(Arc::clone(&task_list));
        let mut args = HashMap::new();
        args.insert("id".to_string(), serde_json::Value::from(1u64));
        let result = tool.run(&args, &ToolRunCtx::default()).await.unwrap();
        assert_eq!(result["status"], "in_progress");
    }

    #[test]
    fn test_descriptor() {
        let task_list = Arc::new(Mutex::new(TaskList::new()));
        let tool = StartTaskTool::new(task_list);
        assert_eq!(tool.descriptor().function.name, "start_task");
    }
}
