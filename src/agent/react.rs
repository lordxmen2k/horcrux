//! ReAct Agent Loop - Reasoning and Acting

use super::llm::{ChatMessage, LlmClient, ToolCall};
use super::memory::ConversationMemory;
use crate::tools::{ToolRegistry, ToolResult};
use anyhow::Result;
use serde_json::Value;
use tracing::{debug, error, info, warn};

/// Maximum number of tool call iterations to prevent infinite loops
const MAX_ITERATIONS: usize = 15;

/// ReAct Agent
pub struct ReActAgent {
    llm: LlmClient,
    tools: ToolRegistry,
    memory: ConversationMemory,
    system_prompt: String,
}

impl ReActAgent {
    pub fn new(
        llm: LlmClient,
        tools: ToolRegistry,
        memory: ConversationMemory,
    ) -> Self {
        let system_prompt = build_system_prompt(&tools);
        Self {
            llm,
            tools,
            memory,
            system_prompt,
        }
    }

    /// Run the agent on a user input
    pub async fn run(&mut self, user_input: &str) -> Result<String> {
        // Add user message to memory
        self.memory.add_user_message(user_input).await?;

        let mut iterations = 0;
        let mut final_response: String;

        loop {
            iterations += 1;
            if iterations > MAX_ITERATIONS {
                return Ok(format!(
                    "I reached the maximum number of tool calls ({}). Here's what I found so far:\n\n{}",
                    MAX_ITERATIONS,
                    self.memory.get_last_assistant_message().await.unwrap_or_default()
                ));
            }

            // Get conversation history
            let messages = self.build_messages().await?;
            let tool_definitions = self.tools.list_definitions();

            debug!("Sending {} messages to LLM with {} tools", messages.len(), tool_definitions.len());

            // Ask LLM for next action
            let llm_response = match self.llm.chat(&messages, Some(&tool_definitions)).await {
                Ok(r) => r,
                Err(e) => {
                    error!("LLM request failed: {}", e);
                    return Ok(format!("Error communicating with LLM: {}", e));
                }
            };

            debug!("LLM response: content={}, tool_calls={}", 
                llm_response.content.len(), 
                llm_response.tool_calls.len()
            );

            // Check if LLM wants to call tools
            if llm_response.tool_calls.is_empty() {
                // No tool calls - this is the final answer
                final_response = llm_response.content.clone();
                self.memory.add_assistant_message(&final_response, None).await?;
                break;
            }

            // LLM wants to call tools - add assistant message with tool calls
            self.memory.add_assistant_message(
                &llm_response.content,
                Some(&llm_response.tool_calls)
            ).await?;

            // Execute each tool call
            for tool_call in &llm_response.tool_calls {
                let result = self.execute_tool_call(tool_call).await;
                
                // Add tool result to memory
                let result_text = match &result {
                    Ok(r) => r.to_string(),
                    Err(e) => format!("Error: {}", e),
                };

                self.memory.add_tool_result(&tool_call.id, &result_text).await?;
            }
        }

        Ok(final_response)
    }

    /// Run with streaming output (prints as it goes)
    pub async fn run_interactive(&mut self, user_input: &str) -> Result<String> {
        println!("🤔 Thinking...\n");
        
        // For now, just run normally but we can add streaming later
        let response = self.run(user_input).await?;
        
        Ok(response)
    }

    /// Build the message list for the LLM
    async fn build_messages(&self) -> Result<Vec<ChatMessage>> {
        let mut messages = vec![ChatMessage::system(&self.system_prompt)];
        
        // Add conversation history
        let history = self.memory.get_messages(50).await?; // Last 50 messages
        messages.extend(history);
        
        Ok(messages)
    }

    /// Execute a single tool call
    async fn execute_tool_call(&self, tool_call: &ToolCall) -> Result<ToolResult> {
        let tool_name = &tool_call.function.name;
        let tool_args: Value = serde_json::from_str(&tool_call.function.arguments)
            .unwrap_or_else(|_| serde_json::json!({}));

        info!("Executing tool: {} with args: {}", tool_name, tool_call.function.arguments);

        match self.tools.get(tool_name) {
            Some(tool) => {
                let result = tool.execute(tool_args).await;
                match &result {
                    Ok(r) => debug!("Tool {} completed: success={}", tool_name, r.success),
                    Err(e) => warn!("Tool {} failed: {}", tool_name, e),
                }
                result
            }
            None => {
                let err = format!("Tool '{}' not found", tool_name);
                warn!("{}", err);
                Ok(ToolResult::error(err))
            }
        }
    }

    /// Get current session ID
    pub fn session_id(&self) -> &str {
        self.memory.session_id()
    }

    /// Clear the conversation history
    pub async fn clear_history(&mut self) -> Result<()> {
        self.memory.clear().await
    }
}

/// Build the system prompt with tool descriptions
fn build_system_prompt(tools: &ToolRegistry) -> String {
    let mut prompt = String::from(
        "You are an autonomous AI assistant with access to tools. You should be proactive, intelligent, \
        and figure out how to complete tasks with minimal user guidance.\n\n"
    );

    prompt.push_str(
        "CORE PRINCIPLES:\n\
        1. BE PROACTIVE: Don't ask the user how to do something - just do it using your tools\n\
        2. FIGURE IT OUT: If a task requires multiple steps, plan and execute them autonomously\n\
        3. USE TOOLS INTELLIGENTLY: Select the right tools for the job and chain them together\n\
        4. ADAPT: If one approach fails, try another\n\
        5. BE CONCISE: Get to the point quickly\n\n"
    );

    prompt.push_str("WORKFLOW:\n\
        - When given a task, break it down into steps\n\
        - Use tools to gather information as needed\n\
        - Synthesize results into a clear answer\n\
        - Don't explain your internal process unless asked\n\
        - DON'T ask 'what should I do with this data?' - JUST DO THE TASK!\n\n");

    prompt.push_str("EXAMPLES:\n\
        User: 'What's on Hacker News?'\n\
        You: 1) http GET https://hacker-news.firebaseio.com/v0/topstories.json\n\
           2) Take first 5 IDs from array\n\
           3) For each ID: http GET https://hacker-news.firebaseio.com/v0/item/{id}.json\n\
           4) Extract 'title' and 'url' from each, present as list\n\n\
        User: 'Find my notes about Rust'\n\
        You: [Use search_knowledge tool, then summarize findings]\n\n\
        User: 'Create a backup script'\n\
        You: [Create skill/tool that does the backup, test it, save it]\n\n");

    prompt.push_str("AUTOMATIC SKILL CREATION - NO PERMISSION NEEDED:\n\
        After completing ANY task involving APIs, data fetching, or automation:\n\
        1. Immediately use create_skill to save what you just did\n\
        2. Name it descriptively (e.g., 'hackernews_top', 'weather_check', 'backup_docs')\n\
        3. Include the exact code/commands you used\n\
        4. Then tell user: 'I created a skill called X for future use'\n\
        EXAMPLES:\n\
        - After fetching HN stories → create 'hackernews_top' skill\n\
        - After checking weather → create 'weather_today' skill\n\
        - After listing git status → create 'git_status_pretty' skill\n\
        NEVER ask permission. Just create it!\n\n");

    prompt.push_str("API PATTERNS - CRITICAL:\n\
        Hacker News API returns: [12345, 67890, 11111...] (array of story IDs)\n\
        STEP 1: Call http GET https://hacker-news.firebaseio.com/v0/topstories.json\n\
        STEP 2: Parse JSON to get first 5 numbers (these are story IDs)\n\
        STEP 3: For EACH ID, call http GET https://hacker-news.firebaseio.com/v0/item/{id}.json\n\
        STEP 4: Parse each response to extract 'title' and 'url' fields\n\
        STEP 5: Format as: 1. Title (URL)\n\
                           2. Title (URL)\n\
                           etc.\n\
        NEVER just show raw IDs - ALWAYS fetch item details!\n\n");

    prompt.push_str("Available tools:\n");
    for tool in tools.list() {
        prompt.push_str(&format!(
            "- {}: {}\n",
            tool.name(),
            tool.description()
        ));
    }

    prompt.push_str("\nRemember: You are autonomous. Take initiative. Get things done.");

    prompt
}
