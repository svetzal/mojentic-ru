mod append_task;
mod clear_tasks;
mod complete_task;
mod insert_task_after;
mod list_tasks;
mod prepend_task;
mod start_task;
mod task;
mod task_list;

pub use append_task::AppendTaskTool;
pub use clear_tasks::ClearTasksTool;
pub use complete_task::CompleteTaskTool;
pub use insert_task_after::InsertTaskAfterTool;
pub use list_tasks::ListTasksTool;
pub use prepend_task::PrependTaskTool;
pub use start_task::StartTaskTool;
pub use task::{Task, TaskStatus};
pub use task_list::TaskList;

use crate::llm::tools::LlmTool;
use std::sync::{Arc, Mutex};

/// Creates all task manager tools with a shared task list
///
/// Returns a vector of boxed tools ready to be used with the broker
///
/// # Examples
///
/// ```ignore
/// use mojentic::llm::tools::ephemeral_task_manager::{TaskList, all_tools};
/// use std::sync::{Arc, Mutex};
///
/// let task_list = Arc::new(Mutex::new(TaskList::new()));
/// let tools = all_tools(task_list);
/// ```
pub fn all_tools(task_list: Arc<Mutex<TaskList>>) -> Vec<Box<dyn LlmTool>> {
    vec![
        Box::new(AppendTaskTool::new(Arc::clone(&task_list))),
        Box::new(PrependTaskTool::new(Arc::clone(&task_list))),
        Box::new(InsertTaskAfterTool::new(Arc::clone(&task_list))),
        Box::new(StartTaskTool::new(Arc::clone(&task_list))),
        Box::new(CompleteTaskTool::new(Arc::clone(&task_list))),
        Box::new(ListTasksTool::new(Arc::clone(&task_list))),
        Box::new(ClearTasksTool::new(Arc::clone(&task_list))),
    ]
}
