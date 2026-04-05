//! Task Scheduler for background jobs
//!
//! Supports cron-style scheduling of recurring tasks.

use anyhow::Result;
use chrono::{DateTime, Utc};
use cron::Schedule as CronSchedule;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// A scheduled task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// Cron expression (e.g., "0 0 * * *" for daily at midnight)
    pub cron: String,
    /// The prompt/task to execute
    pub prompt: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub last_run: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    pub run_count: u64,
}

/// Task execution result
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub executed_at: DateTime<Utc>,
    pub success: bool,
    pub output: Option<String>,
    pub error: Option<String>,
}

/// Callback for task execution
pub type TaskExecutor = Arc<dyn Fn(String) -> tokio::task::JoinHandle<Result<String>> + Send + Sync>;

/// Task scheduler
pub struct Scheduler {
    tasks: Arc<RwLock<HashMap<String, ScheduledTask>>>,
    handles: Arc<RwLock<HashMap<String, JoinHandle<()>>>>,
    executor: Option<TaskExecutor>,
    db_path: std::path::PathBuf,
}

impl Scheduler {
    pub fn new(db_path: std::path::PathBuf) -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            handles: Arc::new(RwLock::new(HashMap::new())),
            executor: None,
            db_path,
        }
    }

    /// Set the task executor callback
    pub fn with_executor<F>(mut self, executor: F) -> Self
    where
        F: Fn(String) -> tokio::task::JoinHandle<Result<String>> + Send + Sync + 'static,
    {
        self.executor = Some(Arc::new(executor));
        self
    }

    /// Add a new scheduled task
    pub async fn add_task(&self, task: ScheduledTask) -> Result<()> {
        // Validate cron expression
        let _ = CronSchedule::from_str(&task.cron)
            .map_err(|e| anyhow::anyhow!("Invalid cron expression '{}': {}", task.cron, e))?;

        let mut tasks = self.tasks.write().await;
        
        // Cancel existing task if any
        if tasks.contains_key(&task.id) {
            drop(tasks);
            self.remove_task(&task.id).await?;
            tasks = self.tasks.write().await;
        }

        tasks.insert(task.id.clone(), task.clone());
        drop(tasks);

        // Start the task if enabled
        if task.enabled {
            self.start_task(&task).await?;
        }

        info!("Added scheduled task: {} ({})", task.name, task.cron);
        Ok(())
    }

    /// Remove a scheduled task
    pub async fn remove_task(&self, task_id: &str) -> Result<()> {
        // Cancel the running task
        let mut handles = self.handles.write().await;
        if let Some(handle) = handles.remove(task_id) {
            handle.abort();
        }
        drop(handles);

        // Remove from tasks
        let mut tasks = self.tasks.write().await;
        tasks.remove(task_id);
        
        info!("Removed scheduled task: {}", task_id);
        Ok(())
    }

    /// Start a scheduled task
    async fn start_task(&self, task: &ScheduledTask) -> Result<()> {
        let executor = self.executor.clone()
            .ok_or_else(|| anyhow::anyhow!("No task executor configured"))?;
        
        let schedule = CronSchedule::from_str(&task.cron)?;
        let task_id = task.id.clone();
        let task_prompt = task.prompt.clone();
        let tasks = self.tasks.clone();

        let handle = tokio::spawn(async move {
            let mut upcoming = schedule.upcoming(Utc);
            
            loop {
                // Get next execution time
                let next = match upcoming.next() {
                    Some(t) => t,
                    None => {
                        error!("No upcoming execution time for task {}", task_id);
                        break;
                    }
                };

                // Update next_run in task
                {
                    let mut tasks_lock = tasks.write().await;
                    if let Some(t) = tasks_lock.get_mut(&task_id) {
                        t.next_run = Some(next);
                    }
                }

                // Calculate sleep duration
                let now = Utc::now();
                let duration = next.signed_duration_since(now);
                
                if duration.num_milliseconds() > 0 {
                    info!("Task {} next run at {} (in {})", task_id, next, duration);
                    tokio::time::sleep(tokio::time::Duration::from_secs(
                        duration.num_seconds().max(1) as u64
                    )).await;
                }

                // Check if task still exists and is enabled
                {
                    let tasks_lock = tasks.read().await;
                    if let Some(t) = tasks_lock.get(&task_id) {
                        if !t.enabled {
                            info!("Task {} disabled, stopping", task_id);
                            break;
                        }
                    } else {
                        info!("Task {} removed, stopping", task_id);
                        break;
                    }
                }

                // Execute the task
                info!("Executing scheduled task: {}", task_id);
                let start = Utc::now();
                
                match executor(task_prompt.clone()).await {
                    Ok(Ok(output)) => {
                        info!("Task {} completed successfully", task_id);
                        // Update task stats
                        let mut tasks_lock = tasks.write().await;
                        if let Some(t) = tasks_lock.get_mut(&task_id) {
                            t.last_run = Some(start);
                            t.run_count += 1;
                        }
                    }
                    Ok(Err(e)) => {
                        error!("Task {} failed: {}", task_id, e);
                    }
                    Err(e) => {
                        error!("Task {} panicked: {}", task_id, e);
                    }
                }
            }
        });

        let mut handles = self.handles.write().await;
        handles.insert(task.id.clone(), handle);

        Ok(())
    }

    /// Enable a task
    pub async fn enable_task(&self, task_id: &str) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(task_id) {
            if !task.enabled {
                task.enabled = true;
                drop(tasks);
                let task = self.tasks.read().await.get(task_id).cloned()
                    .ok_or_else(|| anyhow::anyhow!("Task not found"))?;
                self.start_task(&task).await?;
                info!("Enabled scheduled task: {}", task_id);
            }
        }
        Ok(())
    }

    /// Disable a task
    pub async fn disable_task(&self, task_id: &str) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get_mut(task_id) {
            task.enabled = false;
        }
        drop(tasks);

        // Cancel the running task
        let mut handles = self.handles.write().await;
        if let Some(handle) = handles.remove(task_id) {
            handle.abort();
        }

        info!("Disabled scheduled task: {}", task_id);
        Ok(())
    }

    /// List all tasks
    pub async fn list_tasks(&self) -> Vec<ScheduledTask> {
        let tasks = self.tasks.read().await;
        tasks.values().cloned().collect()
    }

    /// Get a specific task
    pub async fn get_task(&self, task_id: &str) -> Option<ScheduledTask> {
        let tasks = self.tasks.read().await;
        tasks.get(task_id).cloned()
    }

    /// Shutdown all tasks
    pub async fn shutdown(&self) {
        let mut handles = self.handles.write().await;
        for (id, handle) in handles.drain() {
            info!("Shutting down task: {}", id);
            handle.abort();
        }
    }
}

/// Create a scheduler tool for the agent to manage scheduled tasks
pub mod tool {
    use super::*;
    use crate::tools::{Tool, ToolResult};
    use async_trait::async_trait;

    /// Tool for scheduling tasks
    pub struct ScheduleTaskTool {
        scheduler: Arc<Scheduler>,
    }

    impl ScheduleTaskTool {
        pub fn new(scheduler: Arc<Scheduler>) -> Self {
            Self { scheduler }
        }
    }

    #[async_trait]
    impl Tool for ScheduleTaskTool {
        fn name(&self) -> &str {
            "schedule_task"
        }

        fn description(&self) -> &str {
            "Schedule a recurring task using cron syntax. \
             The task will be executed automatically at the specified times. \
             Cron format: 'minute hour day month weekday' (e.g., '0 9 * * 1' = every Monday at 9am)"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name for the scheduled task"
                    },
                    "cron": {
                        "type": "string",
                        "description": "Cron expression (e.g., '0 0 * * *' for daily at midnight, '0 */6 * * *' every 6 hours)"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "The prompt/task to execute on schedule"
                    },
                    "description": {
                        "type": "string",
                        "description": "Optional description of what this task does"
                    }
                },
                "required": ["name", "cron", "prompt"]
            })
        }

        async fn execute(&self, args: serde_json::Value) -> Result<ToolResult> {
            let name = args["name"].as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing 'name' parameter"))?;
            let cron = args["cron"].as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing 'cron' parameter"))?;
            let prompt = args["prompt"].as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing 'prompt' parameter"))?;
            let description = args["description"].as_str();

            let task = ScheduledTask {
                id: format!("task_{}_{}", name.to_lowercase().replace(' ', "_"), Utc::now().timestamp()),
                name: name.to_string(),
                description: description.map(String::from),
                cron: cron.to_string(),
                prompt: prompt.to_string(),
                enabled: true,
                created_at: Utc::now(),
                last_run: None,
                next_run: None,
                run_count: 0,
            };

            self.scheduler.add_task(task.clone()).await?;

            Ok(ToolResult::success(format!(
                "✅ Scheduled task '{}' created (ID: {}).\nSchedule: {}\nNext runs: {}",
                name,
                task.id,
                cron,
                CronSchedule::from_str(cron)?.upcoming(Utc).take(3)
                    .map(|t| t.to_rfc3339())
                    .collect::<Vec<_>>()
                    .join(", ")
            )))
        }
    }

    /// Tool for listing scheduled tasks
    pub struct ListScheduledTasksTool {
        scheduler: Arc<Scheduler>,
    }

    impl ListScheduledTasksTool {
        pub fn new(scheduler: Arc<Scheduler>) -> Self {
            Self { scheduler }
        }
    }

    #[async_trait]
    impl Tool for ListScheduledTasksTool {
        fn name(&self) -> &str {
            "list_scheduled_tasks"
        }

        fn description(&self) -> &str {
            "List all scheduled tasks and their status"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {}
            })
        }

        async fn execute(&self, _args: serde_json::Value) -> Result<ToolResult> {
            let tasks = self.scheduler.list_tasks().await;

            if tasks.is_empty() {
                return Ok(ToolResult::success("No scheduled tasks."));
            }

            let mut output = format!("📅 {} scheduled task(s):\n\n", tasks.len());
            
            for task in tasks {
                let status = if task.enabled { "🟢" } else { "🔴" };
                output.push_str(&format!(
                    "{} {} (ID: {})\n  Schedule: {}\n  Prompt: {}...\n  Last run: {}\n  Next run: {}\n  Runs: {}\n\n",
                    status,
                    task.name,
                    task.id,
                    task.cron,
                    &task.prompt[..task.prompt.len().min(50)],
                    task.last_run.map(|t| t.to_rfc3339()).unwrap_or_else(|| "Never".to_string()),
                    task.next_run.map(|t| t.to_rfc3339()).unwrap_or_else(|| "-".to_string()),
                    task.run_count
                ));
            }

            Ok(ToolResult::success(output))
        }
    }
}
