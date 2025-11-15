use super::task::{Task, TaskStatus};
use crate::error::{MojenticError, Result};

/// Manages a list of tasks for the ephemeral task manager
///
/// This structure provides methods for adding, starting, completing, and listing tasks.
/// Tasks follow a state machine that transitions from Pending through InProgress to Completed.
#[derive(Debug, Clone)]
pub struct TaskList {
    tasks: Vec<Task>,
    next_id: usize,
}

impl TaskList {
    /// Creates a new empty task list
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            next_id: 1,
        }
    }

    /// Claims the next available ID and increments the counter
    fn claim_next_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Appends a new task to the end of the list
    ///
    /// Returns the newly created task with Pending status
    pub fn append_task(&mut self, description: String) -> Task {
        let id = self.claim_next_id();
        let task = Task::new(id, description);
        self.tasks.push(task.clone());
        task
    }

    /// Prepends a new task to the beginning of the list
    ///
    /// Returns the newly created task with Pending status
    pub fn prepend_task(&mut self, description: String) -> Task {
        let id = self.claim_next_id();
        let task = Task::new(id, description);
        self.tasks.insert(0, task.clone());
        task
    }

    /// Inserts a new task after an existing task with the given ID
    ///
    /// Returns the newly created task or an error if the existing task is not found
    pub fn insert_task_after(&mut self, existing_task_id: usize, description: String) -> Result<Task> {
        let position = self
            .tasks
            .iter()
            .position(|t| t.id == existing_task_id)
            .ok_or_else(|| MojenticError::ToolError(format!("No task with ID '{}' exists", existing_task_id)))?;

        let id = self.claim_next_id();
        let task = Task::new(id, description);
        self.tasks.insert(position + 1, task.clone());
        Ok(task)
    }

    /// Starts a task by changing its status from Pending to InProgress
    ///
    /// Returns an error if the task is not found or not in Pending status
    pub fn start_task(&mut self, task_id: usize) -> Result<Task> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| MojenticError::ToolError(format!("No task with ID '{}' exists", task_id)))?;

        if task.status != TaskStatus::Pending {
            return Err(MojenticError::ToolError(format!(
                "Task '{}' cannot be started because it is not in PENDING status",
                task_id
            )));
        }

        task.status = TaskStatus::InProgress;
        Ok(task.clone())
    }

    /// Completes a task by changing its status from InProgress to Completed
    ///
    /// Returns an error if the task is not found or not in InProgress status
    pub fn complete_task(&mut self, task_id: usize) -> Result<Task> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| MojenticError::ToolError(format!("No task with ID '{}' exists", task_id)))?;

        if task.status != TaskStatus::InProgress {
            return Err(MojenticError::ToolError(format!(
                "Task '{}' cannot be completed because it is not in IN_PROGRESS status",
                task_id
            )));
        }

        task.status = TaskStatus::Completed;
        Ok(task.clone())
    }

    /// Returns all tasks in the list
    pub fn list_tasks(&self) -> Vec<Task> {
        self.tasks.clone()
    }

    /// Clears all tasks from the list
    ///
    /// Returns the number of tasks that were cleared
    pub fn clear_tasks(&mut self) -> usize {
        let count = self.tasks.len();
        self.tasks.clear();
        count
    }
}

impl Default for TaskList {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_task_list() {
        let task_list = TaskList::new();
        assert_eq!(task_list.list_tasks().len(), 0);
    }

    #[test]
    fn test_append_task() {
        let mut task_list = TaskList::new();
        let task = task_list.append_task("Test task".to_string());

        assert_eq!(task.id, 1);
        assert_eq!(task.description, "Test task");
        assert_eq!(task.status, TaskStatus::Pending);

        let tasks = task_list.list_tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, task.id);
    }

    #[test]
    fn test_prepend_task() {
        let mut task_list = TaskList::new();
        task_list.append_task("Existing task".to_string());
        let task = task_list.prepend_task("New task".to_string());

        let tasks = task_list.list_tasks();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, task.id);
        assert_eq!(tasks[0].description, "New task");
    }

    #[test]
    fn test_insert_task_after() {
        let mut task_list = TaskList::new();
        let task1 = task_list.append_task("Task 1".to_string());
        task_list.append_task("Task 3".to_string());

        let task2 = task_list.insert_task_after(task1.id, "Task 2".to_string()).unwrap();

        let tasks = task_list.list_tasks();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[1].id, task2.id);
        assert_eq!(tasks[1].description, "Task 2");
    }

    #[test]
    fn test_insert_task_after_nonexistent() {
        let mut task_list = TaskList::new();
        let result = task_list.insert_task_after(999, "Task".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_start_task() {
        let mut task_list = TaskList::new();
        let task = task_list.append_task("Task 1".to_string());

        let started = task_list.start_task(task.id).unwrap();
        assert_eq!(started.status, TaskStatus::InProgress);
    }

    #[test]
    fn test_start_non_pending_task() {
        let mut task_list = TaskList::new();
        let task = task_list.append_task("Task 1".to_string());
        task_list.start_task(task.id).unwrap();

        let result = task_list.start_task(task.id);
        assert!(result.is_err());
    }

    #[test]
    fn test_complete_task() {
        let mut task_list = TaskList::new();
        let task = task_list.append_task("Task 1".to_string());
        task_list.start_task(task.id).unwrap();

        let completed = task_list.complete_task(task.id).unwrap();
        assert_eq!(completed.status, TaskStatus::Completed);
    }

    #[test]
    fn test_complete_non_in_progress_task() {
        let mut task_list = TaskList::new();
        let task = task_list.append_task("Task 1".to_string());

        let result = task_list.complete_task(task.id);
        assert!(result.is_err());
    }

    #[test]
    fn test_clear_tasks() {
        let mut task_list = TaskList::new();
        task_list.append_task("Task 1".to_string());
        task_list.append_task("Task 2".to_string());

        let count = task_list.clear_tasks();
        assert_eq!(count, 2);
        assert_eq!(task_list.list_tasks().len(), 0);
    }

    #[test]
    fn test_maintain_task_ids() {
        let mut task_list = TaskList::new();
        let task1 = task_list.append_task("Task 1".to_string());
        let task2 = task_list.append_task("Task 2".to_string());

        task_list.start_task(task1.id).unwrap();
        task_list.complete_task(task1.id).unwrap();

        let task3 = task_list.append_task("Task 3".to_string());

        let tasks = task_list.list_tasks();
        assert_eq!(tasks.len(), 3);
        assert!(tasks.iter().any(|t| t.id == task1.id && t.status == TaskStatus::Completed));
        assert!(tasks.iter().any(|t| t.id == task2.id && t.status == TaskStatus::Pending));
        assert!(tasks.iter().any(|t| t.id == task3.id && t.status == TaskStatus::Pending));
    }
}
