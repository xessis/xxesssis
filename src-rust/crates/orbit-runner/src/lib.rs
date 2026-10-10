use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunnerTaskStatus {
    Pending,
    Running,
    Success,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerTask {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub status: RunnerTaskStatus,
    pub output: Option<String>,
    pub error: Option<String>,
}

impl RunnerTask {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            description: description.into(),
            status: RunnerTaskStatus::Pending,
            output: None,
            error: None,
        }
    }

    pub fn start(&mut self) {
        self.status = RunnerTaskStatus::Running;
    }

    pub fn succeed(&mut self, output: impl Into<String>) {
        self.status = RunnerTaskStatus::Success;
        self.output = Some(output.into());
    }

    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = RunnerTaskStatus::Failed;
        self.error = Some(error.into());
    }

    pub fn cancel(&mut self) {
        self.status = RunnerTaskStatus::Cancelled;
    }
}

pub struct TaskManager {
    tasks: Vec<RunnerTask>,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskManager {
    pub fn new() -> Self {
        Self { tasks: Vec::new() }
    }

    pub fn add_task(&mut self, task: RunnerTask) {
        self.tasks.push(task);
    }

    pub fn get_task_mut(&mut self, id: Uuid) -> Option<&mut RunnerTask> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    pub fn list_tasks(&self) -> &[RunnerTask] {
        &self.tasks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_lifecycle() {
        let mut mgr = TaskManager::new();
        let task = RunnerTask::new("Build crate", "Runs cargo build");
        let id = task.id;
        mgr.add_task(task);

        let t = mgr.get_task_mut(id).unwrap();
        assert_eq!(t.status, RunnerTaskStatus::Pending);

        t.start();
        assert_eq!(t.status, RunnerTaskStatus::Running);

        t.succeed("Build completed successfully in 0.5s");
        assert_eq!(t.status, RunnerTaskStatus::Success);
        assert_eq!(
            t.output.as_deref(),
            Some("Build completed successfully in 0.5s")
        );
    }
}
