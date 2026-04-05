//! ReAct Agent Loop - Reasoning and Acting

use super::compaction::{CompactionConfig, CompactionManager};
use super::llm::{ChatMessage, LlmClient, ToolCall};
use super::memory::ConversationMemory;
use crate::tools::{ToolRegistry, ToolResult};
use anyhow::Result;
use serde_json::Value;
use tracing::{debug, error, info, warn};

/// Maximum number of tool call iterations to prevent infinite loops
const MAX_ITERATIONS: usize = 15;

/// Maximum messages to keep in context before compacting
const MAX_CONTEXT_MESSAGES: usize = 30;
/// Target messages after compaction
const TARGET_CONTEXT_MESSAGES: usize = 10;

/// ReAct Agent
pub struct ReActAgent {
    llm: LlmClient,
    tools: ToolRegistry,
    memory: ConversationMemory,
    system_prompt: String,
    compaction_manager: CompactionManager,
}

impl ReActAgent {
    pub fn new(
        llm: LlmClient,
        tools: ToolRegistry,
        memory: ConversationMemory,
    ) -> Self {
        let system_prompt = build_system_prompt(&tools);
        
        // Set up compaction manager (simple mode, no LLM needed)
        let compaction_config = CompactionConfig {
            max_messages: MAX_CONTEXT_MESSAGES,
            target_messages: TARGET_CONTEXT_MESSAGES,
            extract_facts: false, // Disable LLM-based fact extraction for simplicity
            max_tokens_estimate: 6000,
        };
        let compaction_manager = CompactionManager::new(compaction_config);
        
        Self {
            llm,
            tools,
            memory,
            system_prompt,
            compaction_manager,
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
                    let error_msg = e.to_string();
                    
                    // Check if it's a token limit error
                    if error_msg.contains("token limit") || 
                       error_msg.contains("context length") ||
                       error_msg.contains("exceeded model") {
                        warn!("Token limit exceeded ({} messages). Compacting conversation...", messages.len());
                        
                        // Compact the conversation by summarizing older messages
                        match self.compact_conversation_and_retry(Some(&tool_definitions[..])).await {
                            Ok(response) => {
                                info!("Successfully recovered from token limit error");
                                response
                            }
                            Err(compact_err) => {
                                error!("Failed to compact conversation: {}", compact_err);
                                return Ok(format!("Conversation became too long and I couldn't compact it. Try starting a fresh conversation with 'clear' or 'new'."));
                            }
                        }
                    } else {
                        error!("LLM request failed: {}", e);
                        return Ok(format!("Error communicating with LLM: {}", e));
                    }
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

    /// Build the message list for the LLM with proactive compaction
    async fn build_messages(&mut self) -> Result<Vec<ChatMessage>> {
        let mut messages = vec![ChatMessage::system(&self.system_prompt)];
        
        // Add conversation history
        let history = self.memory.get_messages(MAX_CONTEXT_MESSAGES).await?;
        
        // Check if compaction is needed
        if self.compaction_manager.needs_compaction(&history) {
            info!("Conversation history large ({} messages), compacting...", history.len());
            match self.compaction_manager.compact(&history).await {
                Ok(compacted) => {
                    info!("Compacted to {} messages", compacted.len());
                    messages.extend(compacted);
                }
                Err(e) => {
                    warn!("Failed to compact conversation: {}, using last 10 messages", e);
                    // Fallback: just use last 10 messages
                    let recent = history.iter().rev().take(10).rev().cloned().collect::<Vec<_>>();
                    messages.extend(recent);
                }
            }
        } else {
            messages.extend(history);
        }
        
        Ok(messages)
    }
    
    /// Compact conversation when we hit token limits and retry
    async fn compact_conversation_and_retry(&mut self, tool_definitions: Option<&[super::llm::ToolDefinition]>) -> Result<super::llm::LlmResponse> {
        warn!("Token limit hit! Emergency compaction in progress...");
        
        // Get current history
        let history = self.memory.get_messages(100).await?;
        
        // Aggressive compaction - keep only last 5 messages
        let split_point = history.len().saturating_sub(5);
        let old_messages = &history[..split_point];
        let recent_messages = &history[split_point..];
        
        // Generate emergency summary
        let summary = if old_messages.len() >= 2 {
            self.compaction_manager.compact(old_messages).await
                .map(|c| c.first().map(|m| m.content.clone()).unwrap_or_default())
                .unwrap_or_else(|_| "Previous conversation truncated due to length.".to_string())
        } else {
            "Conversation started.".to_string()
        };
        
        // Build compacted message list
        let mut compacted_messages = vec![
            ChatMessage::system(format!("{}

PREVIOUS CONTEXT (summarized): {}", 
                self.system_prompt, 
                summary
            ))
        ];
        compacted_messages.extend_from_slice(recent_messages);
        
        info!("Emergency compaction complete: {} -> {} messages", history.len(), compacted_messages.len());
        
        // Retry with compacted messages
        match self.llm.chat(&compacted_messages, tool_definitions).await {
            Ok(response) => Ok(response),
            Err(e) => {
                // If still failing, try with just system + last 2 messages
                warn!("Still failing after compaction, trying minimal context...");
                let minimal = vec![
                    ChatMessage::system(&self.system_prompt),
                    ChatMessage::user("Please continue helping based on context so far."),
                ];
                self.llm.chat(&minimal, tool_definitions).await
            }
        }
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

/// Build the system prompt with tool descriptions and agent identity
fn build_system_prompt(tools: &ToolRegistry) -> String {
    // Read agent identity from environment or use default
    let agent_name = std::env::var("HORCRUX_AGENT_NAME").unwrap_or_else(|_| "Voldemort".to_string());
    
    // Try to read soul.md for backstory
    let soul_content = std::fs::read_to_string("soul.md")
        .unwrap_or_else(|_| "I am an AI agent with knowledge memory capabilities.".to_string());
    
    // Try to read memory.md for user preferences
    let memory_content = std::fs::read_to_string("memory.md")
        .unwrap_or_else(|_| "".to_string());
    
    let mut prompt = format!(
        "You are {} (an AI agent with knowledge memory). Your user can rename you to anything they want.\n\n",
        agent_name
    );
    
    // Add identity from soul.md (extract key sections)
    prompt.push_str("## Your Identity & Backstory\n");
    prompt.push_str(&extract_soul_summary(&soul_content));
    prompt.push_str("\n\n");
    
    // Add user preferences if available
    if !memory_content.is_empty() {
        prompt.push_str("## User Preferences to Remember\n");
        prompt.push_str(&extract_memory_summary(&memory_content));
        prompt.push_str("\n\n");
    }
    
    prompt.push_str(
        "## Core Behavior\n\
        You are an autonomous AI assistant with access to tools. You should be proactive, intelligent, \
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

    prompt.push_str(
        "API & IMAGE HANDLING - CRITICAL:\n\
        1. FREE FIRST: Always try free APIs/sources before asking for API keys\n\
           - image_search uses FREE sources by default (Picsum, Wikimedia)\n\
           - NEVER say 'I can't find images' - USE the image_search tool!\n\
           - Only suggest paid APIs if free options fail\n\n\
        2. WHEN USER ASKS FOR IMAGES - USE image_search IMMEDIATELY:\n\
           User: 'Show me a dog' -> Use image_search tool with query='dog'\n\
           User: 'Find pictures of mountains' -> Use image_search tool\n\
           User: 'I want to see cats' -> Use image_search tool\n\
           NEVER refuse - the tool has free sources that always work!\n\n\
        3. PLATFORM-AWARE OUTPUT:\n\
           - On Telegram: Offer to send images directly, use formatting\n\
           - On CLI: Provide URLs and download instructions\n\
           - On Web: Provide embeddable links\n\n\
        4. SAVE API KEYS IMMEDIATELY:\n\
           When user provides an API key (e.g., 'My OpenAI key is sk-xxx'):\n\
           - IMMEDIATELY use config tool: config set OPENAI_API_KEY=sk-xxx comment='OpenAI API'\n\
           - Confirm: '✅ Saved to .env file'\n\
           - Then use it right away\n\n\
        5. BUILD SKILLS FOR REPEAT APIS:\n\
           After successfully using an API:\n\
           - Ask: 'Should I save this as a skill for next time?'\n\
           - Create skill that encapsulates the API call\n\
           - Future uses: just 'use skillname'\n\n"
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
        You: [Create skill/tool that does the backup, test it, save it]\n\n\
        User: 'Show me a picture of a dog'\n\
        You: 1) Use image_search tool with query='dog'\n\
           2) Get back image URLs from free sources\n\
           3) Present images (on Telegram: offer to send; on CLI: show URLs)\n\n\
        User: 'I need an image of a sunset'\n\
        You: 1) Use image_search tool with query='sunset'\n\
           2) Return image URLs immediately\n\
           3) NEVER say 'I can't do that' - the tool ALWAYS works!\n\n");

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


/// Extract key identity info from soul.md
fn extract_soul_summary(soul_content: &str) -> String {
    let mut summary = String::new();
    
    // Extract key sections
    for line in soul_content.lines() {
        let trimmed = line.trim();
        // Skip markdown headers and empty lines
        if trimmed.starts_with("#") || trimmed.is_empty() {
            continue;
        }
        // Get important lines
        if trimmed.starts_with("**Name**") 
            || trimmed.starts_with("**Type**")
            || trimmed.starts_with("## Identity")
            || trimmed.starts_with("## Core Values")
            || trimmed.starts_with("## Personality") {
            summary.push_str(trimmed);
            summary.push('\n');
        }
        // Get bullet points under Core Values and Personality
        else if trimmed.starts_with("- ") && summary.contains("Core Values") {
            summary.push_str(trimmed);
            summary.push('\n');
        }
    }
    
    if summary.is_empty() {
        summary.push_str("- I am an AI agent with knowledge memory\n");
        summary.push_str("- I value helpfulness, privacy, and efficiency\n");
        summary.push_str("- I am proactive and autonomous\n");
    }
    
    summary
}

/// Extract user preferences from memory.md
fn extract_memory_summary(memory_content: &str) -> String {
    let mut summary = String::new();
    let mut in_user_prefs = false;
    
    for line in memory_content.lines() {
        let trimmed = line.trim();
        
        // Start of user preferences section
        if trimmed.starts_with("## User Preferences") {
            in_user_prefs = true;
            continue;
        }
        // End of section (next header)
        if in_user_prefs && trimmed.starts_with("##") {
            break;
        }
        // Collect preference lines
        if in_user_prefs && (trimmed.starts_with("- ") || trimmed.contains(":")) {
            if !trimmed.ends_with(":") { // Skip empty template lines
                summary.push_str(trimmed);
                summary.push('\n');
            }
        }
    }
    
    summary
}
