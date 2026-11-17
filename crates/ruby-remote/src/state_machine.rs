//! In-memory deterministic task manager and state machine.

use crate::types::{RemoteTask, TaskState};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone)]
pub struct TaskManager {
    tasks: Arc<Mutex<Vec<RemoteTask>>>,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Creates a new task and returns its unique ID.
    pub fn create_task(&self, sender_id: i64, prompt: &str) -> RemoteTask {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let id = format!("task-{}", &Uuid::new_v4().to_string()[..8]);
        let task = RemoteTask {
            id,
            sender_id,
            raw_prompt: prompt.to_string(),
            state: TaskState::Queued,
            created_at_secs: now,
            updated_at_secs: now,
            result: None,
            error: None,
        };

        if let Ok(mut lock) = self.tasks.lock() {
            lock.push(task.clone());
        }

        task
    }

    /// Updates task state and optional result/error.
    pub fn update_task_state(
        &self,
        task_id: &str,
        new_state: TaskState,
        result: Option<String>,
        error: Option<String>,
    ) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Ok(mut lock) = self.tasks.lock() {
            if let Some(t) = lock.iter_mut().find(|t| t.id == task_id) {
                t.state = new_state;
                t.updated_at_secs = now;
                if result.is_some() {
                    t.result = result;
                }
                if error.is_some() {
                    t.error = error;
                }
                return true;
            }
        }
        false
    }

    /// Cancels all active and queued tasks (e.g. on /stop). Returns number of cancelled tasks.
    pub fn cancel_all_active(&self) -> usize {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut count = 0;
        if let Ok(mut lock) = self.tasks.lock() {
            for t in lock.iter_mut() {
                if matches!(
                    t.state,
                    TaskState::Queued | TaskState::Running | TaskState::NeedsInput
                ) {
                    t.state = TaskState::Cancelled;
                    t.updated_at_secs = now;
                    t.error = Some("Cancelled via /stop command".to_string());
                    count += 1;
                }
            }
        }
        count
    }

    /// Returns the recent tasks up to `limit`.
    pub fn get_recent_tasks(&self, limit: usize) -> Vec<RemoteTask> {
        if let Ok(lock) = self.tasks.lock() {
            let mut list = lock.clone();
            list.reverse();
            list.truncate(limit);
            list
        } else {
            Vec::new()
        }
    }

    /// Returns the currently active (running or queued) task count.
    pub fn active_task_count(&self) -> usize {
        if let Ok(lock) = self.tasks.lock() {
            lock.iter()
                .filter(|t| {
                    matches!(
                        t.state,
                        TaskState::Queued | TaskState::Running | TaskState::NeedsInput
                    )
                })
                .count()
        } else {
            0
        }
    }
}
