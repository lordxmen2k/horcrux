//! Subagent System - Parallel task execution
//!
//! Allows the agent to spawn subagents that run tasks in parallel
//! and return results for aggregation.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{error, info};

/// Configuration for a subagent task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentTask {
    /// Unique task ID
    pub id: String,
    /// Task description/prompt
    pub prompt: String,
    /// Context to pass to the subagent
    pub context: Option<String>,
    /// Maximum iterations allowed
    pub max_iterations: usize,
    /// Tools the subagent can use
    pub allowed_tools: Option<Vec<String>>,
}

impl SubagentTask {
    pub fn new(id: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            prompt: prompt.into(),
            context: None,
            max_iterations: 10,
            allowed_tools: None,
        }
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }
}

/// Result from a subagent task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentResult {
    pub task_id: String,
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    pub iterations_used: usize,
    pub tools_used: Vec<String>,
}

/// Subagent executor that runs tasks in parallel
pub struct SubagentExecutor {
    agent_factory: Arc<dyn Fn() -> crate::agent::Agent + Send + Sync>,
}

impl SubagentExecutor {
    pub fn new<F>(factory: F) -> Self
    where
        F: Fn() -> crate::agent::Agent + Send + Sync + 'static,
    {
        Self {
            agent_factory: Arc::new(factory),
        }
    }

    /// Execute a single subagent task
    pub async fn execute(&self, task: SubagentTask) -> SubagentResult {
        info!("🧬 Subagent [{}] starting...", task.id);

        let mut agent = (self.agent_factory)();
        
        // Build the full prompt with context
        let full_prompt = if let Some(ctx) = &task.context {
            format!("Context: {}\n\nTask: {}", ctx, task.prompt)
        } else {
            task.prompt.clone()
        };

        let start_time = std::time::Instant::now();
        
        match agent.run(&full_prompt).await {
            Ok(output) => {
                let elapsed = start_time.elapsed();
                info!("🧬 Subagent [{}] completed in {:?}", task.id, elapsed);
                
                SubagentResult {
                    task_id: task.id,
                    success: true,
                    output,
                    error: None,
                    iterations_used: 0, // Would need to track from agent
                    tools_used: Vec::new(), // Would need to track from agent
                }
            }
            Err(e) => {
                error!("🧬 Subagent [{}] failed: {}", task.id, e);
                
                SubagentResult {
                    task_id: task.id,
                    success: false,
                    output: String::new(),
                    error: Some(e.to_string()),
                    iterations_used: 0,
                    tools_used: Vec::new(),
                }
            }
        }
    }

    /// Execute multiple tasks in parallel
    pub async fn execute_parallel(&self, tasks: Vec<SubagentTask>) -> Vec<SubagentResult> {
        let mut handles: Vec<JoinHandle<SubagentResult>> = Vec::new();

        for task in tasks {
            let executor = Self {
                agent_factory: self.agent_factory.clone(),
            };
            
            let handle = tokio::spawn(async move {
                executor.execute(task).await
            });
            
            handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => {
                    error!("Subagent task panicked: {}", e);
                    results.push(SubagentResult {
                        task_id: "unknown".to_string(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("Task panicked: {}", e)),
                        iterations_used: 0,
                        tools_used: Vec::new(),
                    });
                }
            }
        }

        results
    }

    /// Execute tasks with a limit on concurrent execution
    pub async fn execute_limited(&self, tasks: Vec<SubagentTask>, max_concurrent: usize) -> Vec<SubagentResult> {
        use tokio::sync::Semaphore;
        use std::sync::Arc as StdArc;

        let semaphore = StdArc::new(Semaphore::new(max_concurrent));
        let mut handles: Vec<JoinHandle<SubagentResult>> = Vec::new();

        for task in tasks {
            let sem = semaphore.clone();
            let executor = Self {
                agent_factory: self.agent_factory.clone(),
            };
            
            let handle = tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                executor.execute(task).await
            });
            
            handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => {
                    error!("Subagent task panicked: {}", e);
                    results.push(SubagentResult {
                        task_id: "unknown".to_string(),
                        success: false,
                        output: String::new(),
                        error: Some(format!("Task panicked: {}", e)),
                        iterations_used: 0,
                        tools_used: Vec::new(),
                    });
                }
            }
        }

        results
    }
}

/// Tool for spawning subagents
pub struct DelegateTaskTool {
    executor: Arc<SubagentExecutor>,
}

impl DelegateTaskTool {
    pub fn new(executor: Arc<SubagentExecutor>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl crate::tools::Tool for DelegateTaskTool {
    fn name(&self) -> &str {
        "delegate_task"
    }

    fn description(&self) -> &str {
        "Delegate a task to a subagent that runs independently. \
         Use this to parallelize work or isolate complex sub-tasks. \
         The subagent has its own context and returns results that can be aggregated."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Unique identifier for this task"
                },
                "prompt": {
                    "type": "string",
                    "description": "The task prompt/instruction for the subagent"
                },
                "context": {
                    "type": "string",
                    "description": "Optional context information to provide to the subagent"
                },
                "max_iterations": {
                    "type": "integer",
                    "description": "Maximum iterations allowed (default: 10)",
                    "default": 10
                }
            },
            "required": ["task_id", "prompt"]
        })
    }

    async fn execute(&self, args: Value) -> Result<crate::tools::ToolResult> {
        let task_id = args["task_id"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'task_id' parameter"))?;
        let prompt = args["prompt"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'prompt' parameter"))?;
        let context = args["context"].as_str();
        let max_iterations = args["max_iterations"].as_u64().unwrap_or(10) as usize;

        let task = SubagentTask {
            id: task_id.to_string(),
            prompt: prompt.to_string(),
            context: context.map(String::from),
            max_iterations,
            allowed_tools: None,
        };

        let result = self.executor.execute(task).await;

        let output = if result.success {
            format!(
                "✅ Subagent [{}] completed successfully\n\nOutput:\n{}",
                result.task_id,
                result.output
            )
        } else {
            format!(
                "❌ Subagent [{}] failed\n\nError: {}",
                result.task_id,
                result.error.unwrap_or_else(|| "Unknown error".to_string())
            )
        };

        Ok(crate::tools::ToolResult::success(output))
    }
}

/// Tool for parallel delegation of multiple tasks
pub struct DelegateParallelTool {
    executor: Arc<SubagentExecutor>,
}

impl DelegateParallelTool {
    pub fn new(executor: Arc<SubagentExecutor>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl crate::tools::Tool for DelegateParallelTool {
    fn name(&self) -> &str {
        "delegate_parallel"
    }

    fn description(&self) -> &str {
        "Delegate multiple tasks to subagents that run in parallel. \
         Use this when you need to process multiple items simultaneously. \
         Results are returned when all tasks complete."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "tasks": {
                    "type": "array",
                    "description": "List of tasks to execute in parallel",
                    "items": {
                        "type": "object",
                        "properties": {
                            "task_id": { "type": "string" },
                            "prompt": { "type": "string" },
                            "context": { "type": "string" }
                        },
                        "required": ["task_id", "prompt"]
                    }
                }
            },
            "required": ["tasks"]
        })
    }

    async fn execute(&self, args: Value) -> Result<crate::tools::ToolResult> {
        let tasks_array = args["tasks"].as_array()
            .ok_or_else(|| anyhow::anyhow!("Missing 'tasks' array"))?;

        let mut tasks = Vec::new();
        for task_value in tasks_array {
            let task_id = task_value["task_id"].as_str()
                .ok_or_else(|| anyhow::anyhow!("Task missing 'task_id'"))?;
            let prompt = task_value["prompt"].as_str()
                .ok_or_else(|| anyhow::anyhow!("Task missing 'prompt'"))?;
            let context = task_value["context"].as_str();

            tasks.push(SubagentTask {
                id: task_id.to_string(),
                prompt: prompt.to_string(),
                context: context.map(String::from),
                max_iterations: 10,
                allowed_tools: None,
            });
        }

        let results = self.executor.execute_parallel(tasks).await;

        let mut output = format!("🧬 Parallel Subagent Results ({} tasks):\n\n", results.len());
        
        for result in results {
            let status = if result.success { "✅" } else { "❌" };
            output.push_str(&format!(
                "{} [{}]: {}\n",
                status,
                result.task_id,
                if result.success { 
                    format!("Completed\n{}", result.output) 
                } else { 
                    format!("Failed: {}", result.error.unwrap_or_default()) 
                }
            ));
            output.push_str("\n---\n\n");
        }

        Ok(crate::tools::ToolResult::success(output))
    }
}
