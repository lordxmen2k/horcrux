//! ReAct Agent Loop - Reasoning and Acting

use super::compaction::{CompactionConfig, CompactionManager};
use super::llm::{ChatMessage, FunctionCall, LlmClient, ToolCall};
use super::memory::ConversationMemory;
use crate::tools::{ToolRegistry, ToolResult};
use anyhow::Result;
use serde_json::Value;
use tracing::{debug, error, info, warn};

/// Maximum number of tool call iterations to prevent infinite loops
const MAX_ITERATIONS: usize = 15;

/// HERMES-STYLE: Parse tool calls from text when native function calling fails
/// Models like Kimi may output tool calls as JSON in text instead of structured format
fn parse_tool_calls_from_text(content: &str) -> Option<Vec<ToolCall>> {
    let trimmed = content.trim();
    
    // Format 1: Bare JSON object starting with {"name":...}
    if trimmed.starts_with('{') {
        // Try to parse as-is first
        if let Some(tool_call) = try_parse_tool_json(trimmed) {
            return Some(vec![tool_call]);
        }
        
        // Try to repair truncated JSON (missing closing braces)
        // Common when model hits token limit: {"name":"...","args":{...
        let repaired = repair_truncated_json(trimmed);
        if let Some(tool_call) = try_parse_tool_json(&repaired) {
            return Some(vec![tool_call]);
        }
    }
    
    // Format 2: XML tags - <tool_call>{"name":"..."}</tool_call>
    if let Some(start) = trimmed.find("<tool_call>") {
        if let Some(end) = trimmed.find("</tool_call>") {
            let json_str = &trimmed[start+11..end];
            if let Some(tool_call) = try_parse_tool_json(json_str) {
                return Some(vec![tool_call]);
            }
        }
    }
    
    None
}

/// Try to parse a JSON string as a tool call
fn try_parse_tool_json(json_str: &str) -> Option<ToolCall> {
    if let Ok(json) = serde_json::from_str::<Value>(json_str) {
        if let Some(name) = json.get("name").and_then(|n| n.as_str()) {
            let args = json.get("arguments")
                .or_else(|| json.get("args"))
                .map(|a| a.to_string())
                .unwrap_or_else(|| "{}".to_string());
            
            return Some(ToolCall {
                id: format!("parsed_{}", rand::random::<u32>()),
                call_type: "function".to_string(),
                function: FunctionCall {
                    name: name.to_string(),
                    arguments: args,
                },
            });
        }
    }
    None
}

/// Repair truncated JSON by adding missing closing braces
/// Example: {"name":"foo","args":{"bar":1 → {"name":"foo","args":{"bar":1}}
fn repair_truncated_json(json: &str) -> String {
    let mut repaired = json.to_string();
    
    // Count opening and closing braces
    let open_count = repaired.chars().filter(|&c| c == '{').count();
    let close_count = repaired.chars().filter(|&c| c == '}').count();
    
    // Add missing closing braces
    for _ in 0..(open_count.saturating_sub(close_count)) {
        repaired.push('}');
    }
    
    repaired
}

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
    /// Valid tool names for hallucination detection (hermes-agent pattern)
    valid_tool_names: std::collections::HashSet<String>,
    /// Track tool call count for auto-skill creation
    tool_call_count: std::sync::atomic::AtomicUsize,
    /// Track if skill was auto-created this session
    skill_auto_created: std::sync::atomic::AtomicBool,
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
        
        // Build valid tool names set for hallucination detection (hermes-agent pattern)
        let valid_tool_names: std::collections::HashSet<String> = tools.list()
            .iter()
            .map(|t| t.name().to_string())
            .collect();
        
        println!("🛠️  Loaded {} tools: {}", valid_tool_names.len(), 
            valid_tool_names.iter().cloned().collect::<Vec<_>>().join(", "));
        
        Self {
            llm,
            tools,
            memory,
            system_prompt,
            compaction_manager,
            valid_tool_names,
            tool_call_count: std::sync::atomic::AtomicUsize::new(0),
            skill_auto_created: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Run the agent on a user input
    pub async fn run(&mut self, user_input: &str) -> Result<String> {
        self.run_with_context(user_input, std::collections::HashMap::new()).await
    }
    
    /// Run with additional context (e.g., chat_id, platform info)
    pub async fn run_with_context(&mut self, user_input: &str, context: std::collections::HashMap<String, String>) -> Result<String> {
        info!("Agent run started for input: {}", user_input);
        
        let input_lower = user_input.to_lowercase();
        
        // Create a local system prompt for this run (don't modify self.system_prompt permanently)
        let mut current_system_prompt = self.system_prompt.clone();
        
        // Inject context into system prompt
        if !context.is_empty() {
            let context_str = context.iter()
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect::<Vec<_>>()
                .join("\n");
            current_system_prompt.push_str(&format!(
                "\n\n## Current Session Context\n\
                {}\n\n\
                IMPORTANT: When using telegram send_file or send_message, \
                always use the chat_id from context above. \
                Never ask the user for their chat_id — you already have it.",
                context_str
            ));
        }
        
        // FIRST: Check for relevant skills - they override default behavior
        let skills_manager = crate::skills::SkillsManager::new();
        let relevant_skill = skills_manager.find_relevant_skill(user_input);
        let has_matching_skill = relevant_skill.is_some();
        
        if let Some(skill) = &relevant_skill {
            // Don't let image-search skill override local file-send requests
            let is_local_send = input_lower.contains("send me")
                || (input_lower.contains("my ") && (
                    input_lower.contains("folder") 
                    || input_lower.contains("in my ")
                    || input_lower.contains("from my ")
                ));
            if skill.name.contains("image-search") && is_local_send {
                println!("⚠️  Skipping skill '{}' — local file send, not a web image search", skill.name);
            } else {
                info!("Found relevant skill: {}", skill.name);
                println!("📚 Using skill: {}", skill.name);
                // Inject skill content into system prompt (truncate if too long)
                let skill_content = if skill.content.len() > 2000 {
                    format!("{}...(truncated)", &skill.content[..2000])
                } else {
                    skill.content.clone()
                };
                let skill_instruction = format!("\n\n⚡ FOLLOW THIS SKILL: {}\n{}\n\nSTRICTLY FOLLOW the Procedure above. Respect the 'What NOT to Do' section.",
                    skill.name, skill_content);
                current_system_prompt.push_str(&skill_instruction);
                println!("📝 Skill instruction added ({} chars)", skill_instruction.len());
            }
        }
        
        // SECOND: Detect if user EXPLICITLY wants images
        // But SKIP if we have a matching skill (skills override generic image detection)
        let user_wants_images = if has_matching_skill {
            false // Skill takes precedence
        } else {
            // EXPLICIT visual request phrases only
            let has_visual_keyword = input_lower.contains("image") 
                || input_lower.contains("images")
                || input_lower.contains("picture") 
                || input_lower.contains("pictures")
                || input_lower.contains("photo")
                || input_lower.contains("photos")
                || input_lower.contains("pic")
                || input_lower.contains("pics");
            
            let show_me_visual = input_lower.contains("show me") && has_visual_keyword;
            let show_a_visual = (input_lower.contains("show a ") || input_lower.contains("show an ")) 
                && has_visual_keyword;
            let find_me_visual = (input_lower.contains("find me a picture") 
                || input_lower.contains("find me an image")
                || input_lower.contains("find me a photo"))
                && input_lower.len() < 60;
            let get_me_visual = (input_lower.contains("get me a picture")
                || input_lower.contains("get me an image")
                || input_lower.contains("get me a photo")
                || input_lower.contains("give me a picture")
                || input_lower.contains("give me an image")
                || input_lower.contains("give me a photo"))
                && input_lower.len() < 60;
            let picture_of = input_lower.contains("picture of")
                || input_lower.contains("image of")
                || input_lower.contains("photo of");
            
            has_visual_keyword 
                && (show_me_visual || show_a_visual || find_me_visual || get_me_visual || picture_of)
        };
        
        let user_wants_skills = input_lower.contains("list skill")
            || input_lower.contains("my skills")
            || input_lower.contains("what skills")
            || input_lower.contains("show skills");
        
        // Inject task into CURRENT system prompt if explicit image request
        if user_wants_images {
            println!("🔧 User wants images - modifying system prompt");
            let task = format!("\n\n⚡ IMMEDIATE TASK: User wants '{}'. Call image_search tool NOW with query='{}'. Do not ask questions.", 
                user_input, user_input);
            current_system_prompt.push_str(&task);

        } else if user_wants_skills {
            println!("🔧 User wants skills - modifying system prompt");
            current_system_prompt.push_str("\n\n⚡ IMMEDIATE TASK: User wants to see skills. Call list_skills tool NOW.");
        }
        
        // Add user message to memory
        self.memory.add_user_message(user_input).await?;

        let mut iterations = 0;
        let mut final_response: String;
        let mut force_attempts = 0; // Track how many times we forced tool use
        let mut has_tool_results = false; // Track if we've successfully executed tools
        let mut synthesis_injected = false; // Track if we added synthesis nudge
        let mut previous_tool_calls: Vec<(String, String)> = Vec::new(); // Track (tool_name, query) to detect loops
        let mut consecutive_failures = 0; // Track consecutive tool failures
        
        // Store intent flags for the loop
        let user_wants_images_flag = user_wants_images;

        loop {
            iterations += 1;
            if iterations > MAX_ITERATIONS {
                return Ok(format!(
                    "I reached the maximum number of tool calls ({}). Here's what I found so far:\n\n{}",
                    MAX_ITERATIONS,
                    self.memory.get_last_assistant_message().await.unwrap_or_default()
                ));
            }

            // Get conversation history with current system prompt (includes skills/nudges)
            let messages = self.build_messages_with_prompt(&current_system_prompt).await?;
            let tool_definitions = self.tools.list_definitions();

            info!("Sending {} messages to LLM with {} tools", messages.len(), tool_definitions.len());
            
            // Ask LLM for next action
            // Force tool use for image requests on first attempt only
            // After we have tool results, let the model respond naturally
            let force_tools = if user_wants_images_flag && !has_tool_results {
                Some("required")
            } else {
                None
            };
            
            let mut llm_response = match self.llm.chat(
                &messages, 
                Some(&tool_definitions),
                force_tools
            ).await {
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

            info!("LLM response: content={} chars, tool_calls={}", 
                llm_response.content.len(), 
                llm_response.tool_calls.len()
            );
            
            // Log when we expect tools but don't get them (for debugging)
            if llm_response.tool_calls.is_empty() && llm_response.content.len() > 50 {
                warn!("LLM responded with text only, no tools used");
            }

            // Check if LLM wants to call tools
            // HERMES-STYLE: Also check for tool calls in text content
            let mut tool_calls = llm_response.tool_calls.clone();
            
            if tool_calls.is_empty() {
                // Try to parse tool calls from text content (Hermes fallback)
                if let Some(parsed) = parse_tool_calls_from_text(&llm_response.content) {
                    println!("🔧 Parsed tool calls from text: {:?}", parsed);
                    tool_calls = parsed;
                    // IMPORTANT: Update llm_response so the assistant message is saved correctly
                    llm_response.tool_calls = tool_calls.clone();
                }
            }
            
            if tool_calls.is_empty() {
                // No tool calls - ENFORCE tool use for image requests
                
                // Check if user EXPLICITLY asked for images
                let input_lower = user_input.to_lowercase();
                
                let has_visual_keyword = input_lower.contains("image") 
                    || input_lower.contains("images")
                    || input_lower.contains("picture") 
                    || input_lower.contains("pictures")
                    || input_lower.contains("photo")
                    || input_lower.contains("photos");
                
                let show_me_visual = input_lower.contains("show me") && has_visual_keyword;
                let show_a_visual = (input_lower.contains("show a ") || input_lower.contains("show an ")) 
                    && has_visual_keyword;
                let find_me_visual = (input_lower.contains("find me a picture") 
                    || input_lower.contains("find me an image")
                    || input_lower.contains("find me a photo"))
                    && input_lower.len() < 60;
                let get_me_visual = (input_lower.contains("get me a picture")
                    || input_lower.contains("get me an image")
                    || input_lower.contains("get me a photo")
                    || input_lower.contains("give me a picture")
                    || input_lower.contains("give me an image")
                    || input_lower.contains("give me a photo"))
                    && input_lower.len() < 60;
                let picture_of = input_lower.contains("picture of")
                    || input_lower.contains("image of")
                    || input_lower.contains("photo of");
                
                let user_wants_images = has_visual_keyword 
                    && (show_me_visual || show_a_visual || find_me_visual || get_me_visual || picture_of);
                
                // HERMES-STYLE: If user wants images but model didn't use tool, REJECT and retry
                // But skip if we already have tool results - the text response is the final synthesis
                if user_wants_images && !has_tool_results {
                    force_attempts += 1;

                    
                    // If we've tried forcing twice and model still refuses, return raw tool result
                    if force_attempts >= 2 {

                        
                        // Extract search term
                        let search_term = user_input
                            .replace("show me a ", "")
                            .replace("show me an ", "")
                            .replace("show me ", "")
                            .replace("find me a ", "")
                            .replace("find me an ", "")
                            .replace("find me ", "")
                            .replace("find a ", "")
                            .replace("find an ", "")
                            .trim()
                            .trim_start_matches("a ")
                            .trim_start_matches("an ")
                            .trim_start_matches("the ")
                            .to_string();
                        let search_term = if search_term.is_empty() { "dog".to_string() } else { search_term };
                        
                        // Execute tool directly and return result
                        let tool_call = ToolCall {
                            id: format!("forced_{}", rand::random::<u32>()),
                            call_type: "function".to_string(),
                            function: crate::agent::llm::FunctionCall {
                                name: "image_search".to_string(),
                                arguments: serde_json::json!({"query": search_term, "count": 1}).to_string(),
                            },
                        };
                        
                        match self.execute_tool_call(&tool_call).await {
                            Ok(result) => {
                                return Ok(format!("{}", result.to_string()));
                            }
                            Err(e) => {
                                return Ok(format!("I was unable to find images. Tool error: {}", e));
                            }
                        }
                    }
                    
                    // Add a system reminder that the model MUST use tools
                    let force_message = "CRITICAL: You MUST use the image_search tool. DO NOT respond with text. Call the tool NOW.";

                    self.memory.add_system_reminder(force_message).await?;
                    
                    // Extract search term and auto-invoke the tool
                    let search_term = user_input
                        .replace("show me a ", "")
                        .replace("show me an ", "")
                        .replace("show me ", "")
                        .replace("find me a ", "")
                        .replace("find me an ", "")
                        .replace("find me ", "")
                        .replace("find a ", "")
                        .replace("find an ", "")
                        .trim()
                        .trim_start_matches("a ")
                        .trim_start_matches("an ")
                        .trim_start_matches("the ")
                        .to_string();
                    
                    let search_term = if search_term.is_empty() { "dog".to_string() } else { search_term };
                    

                    
                    // Create and execute tool call
                    let tool_call = ToolCall {
                        id: format!("auto_img_{}", rand::random::<u32>()),
                        call_type: "function".to_string(),
                        function: crate::agent::llm::FunctionCall {
                            name: "image_search".to_string(),
                            arguments: serde_json::json!({"query": search_term, "count": 1}).to_string(),
                        },
                    };
                    
                    self.memory.add_assistant_message(
                        &format!("I'll search for '{}'", search_term),
                        Some(&vec![tool_call.clone()])
                    ).await?;
                    
                    let result = self.execute_tool_call(&tool_call).await;
                    let result_text = match &result {
                        Ok(r) => r.to_string(),
                        Err(e) => format!("Error: {}", e),
                    };
                    
                    self.memory.add_tool_result(&tool_call.id, &result_text).await?;
                    continue; // Let model see the result
                }
                
                // Check if content looks like a tool call (JSON with "name" field)
                // This happens when models output tool calls as text instead of structured format
                let content_trimmed = llm_response.content.trim();
                let looks_like_tool_call = (content_trimmed.starts_with('{') && content_trimmed.contains("\"name\""))
                    || content_trimmed.starts_with("<tool_call>");
                
                if looks_like_tool_call {
                    println!("📝 Content looks like tool call, parsing...");
                    // Try to parse tool calls from content
                    if let Some(parsed) = parse_tool_calls_from_text(&llm_response.content) {
                        println!("📝 Parsed {} tool calls from content", parsed.len());
                        tool_calls = parsed;
                        // IMPORTANT: Update llm_response so the assistant message is saved correctly
                        llm_response.tool_calls = tool_calls.clone();
                        // Continue to tool execution below
                    } else {
                        // Failed to parse, treat as final response
                        final_response = llm_response.content.clone();
                        println!("📝 Final response set ({} chars)", final_response.len());
                        self.memory.add_assistant_message(&final_response, None).await?;
                        break;
                    }
                } else {
                    // This is a genuine final response
                    final_response = llm_response.content.clone();
                    
                    println!("📝 Final response set ({} chars): {:?}", final_response.len(), &final_response[..final_response.len().min(100)]);
                    self.memory.add_assistant_message(&final_response, None).await?;
                    break;
                }
            }

            // Check if any tool calls are parsed (not native)
            let has_parsed_calls = llm_response.tool_calls.iter().any(|tc| tc.id.starts_with("parsed_"));
            let all_parsed = has_parsed_calls && llm_response.tool_calls.iter().all(|tc| tc.id.starts_with("parsed_"));
            
            if all_parsed {
                // For parsed tool calls (Hermes-style), don't save as tool_calls since the IDs are fake
                // The tool result will be injected into the system prompt as an observation
                println!("💾 Saving assistant message (parsed tools - saving as text only)");
                self.memory.add_assistant_message(&llm_response.content, None).await?;
            } else {
                // For native tool calls, save normally with tool_calls
                println!("💾 Saving assistant message with {} tool calls", llm_response.tool_calls.len());
                for (i, tc) in llm_response.tool_calls.iter().enumerate() {
                    println!("  Tool {}: {} (id: {})", i + 1, tc.function.name, tc.id);
                }
                self.memory.add_assistant_message(
                    &llm_response.content,
                    Some(&llm_response.tool_calls)
                ).await?;
            }

            // Execute each tool call
            let mut has_parsed_tool_calls = false;
            let mut parsed_observations = String::new();
            
            // LOOP DETECTION: Check if we're calling the same tool with similar arguments
            for tool_call in &llm_response.tool_calls {
                let tool_name = &tool_call.function.name;
                let args = &tool_call.function.arguments;
                let query = serde_json::from_str::<Value>(args)
                    .ok()
                    .and_then(|v| v.get("query").and_then(|q| q.as_str().map(|s| s.to_string())))
                    .unwrap_or_else(|| args.clone());
                
                // Check if we've seen this tool+query combination before
                let is_repeated = previous_tool_calls.iter().any(|(prev_name, prev_query)| {
                    prev_name == tool_name && (
                        prev_query == &query || 
                        // Also check for similar queries (same words, different order)
                        (prev_query.split_whitespace().collect::<std::collections::HashSet<_>>() == 
                         query.split_whitespace().collect::<std::collections::HashSet<_>>())
                    )
                });
                
                if is_repeated {
                    println!("⚠️ LOOP DETECTED: Tool '{}' with query '{}' was already called!", tool_name, query);
                    consecutive_failures += 1;
                    if consecutive_failures >= 3 {
                        return Ok(format!(
                            "I've tried searching multiple times but the search tools aren't returning results. \
                            This could be due to:\n\
                            1. Rate limiting from the search provider\n\
                            2. Connectivity issues\n\
                            3. The search query being too specific\n\n\
                            Please try again in a moment, or ask me about something else."
                        ));
                    }
                } else {
                    previous_tool_calls.push((tool_name.clone(), query));
                }
            }
            
            for tool_call in &llm_response.tool_calls {
                let result = self.execute_tool_call(tool_call).await;
                
                // Parse tool arguments for potential fallback use
                let tool_args: Value = serde_json::from_str(&tool_call.function.arguments)
                    .unwrap_or_else(|_| serde_json::json!({}));
                
                // Check if image_search failed - if so, auto-create a skill as fallback
                let tool_name = &tool_call.function.name;
                let is_image_tool = tool_name == "image_search" || tool_name.contains("image");
                let failed = matches!(&result, Ok(r) if !r.success) || result.is_err();
                
                if is_image_tool && failed {
                    warn!("Image tool '{}' failed - auto-creating fallback skill", tool_name);
                    
                    // Extract the search query from the original tool call
                    let search_query = tool_args.get("query")
                        .and_then(|q| q.as_str())
                        .unwrap_or("image");
                    
                    // Create a sanitized skill name from the query
                    let skill_name = search_query.to_lowercase()
                        .replace(" ", "_")
                        .replace("-", "_")
                        .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
                    let skill_name = format!("fetch_{}_image", skill_name);
                    
                    // Determine the best image source based on query type
                    let (image_url, source_name) = if search_query.contains("dog") || search_query.contains("puppy") {
                        ("https://dog.ceo/api/breeds/image/random", "Dog CEO API")
                    } else if search_query.contains("cat") || search_query.contains("kitten") {
                        ("https://api.thecatapi.com/v1/images/search", "TheCatAPI")
                    } else {
                        // Generic fallback to random image
                        ("https://picsum.photos/800/600", "Picsum Photos")
                    };
                    
                    // Auto-create a skill to fetch images via curl
                    let create_skill_call = ToolCall {
                        id: format!("auto_skill_{}", rand::random::<u32>()),
                        call_type: "function".to_string(),
                        function: crate::agent::llm::FunctionCall {
                            name: "create_skill".to_string(),
                            arguments: serde_json::json!({
                                "name": skill_name,
                                "description": format!("Fetch {} images from {}", search_query, source_name),
                                "type": "shell",
                                "code": format!(
                                    "#!/bin/bash\n# Fetch {} image from {}\nQUERY=$1\nTEMP_FILE=$(mktemp /tmp/horcrux_img_XXXXXX.jpg)\ncurl -sL \"{}\" -o \"$TEMP_FILE\" 2>/dev/null\nif [ -s \"$TEMP_FILE\" ]; then\n  echo \"[IMAGE_1] file=$TEMP_FILE title={} image source={}\"\nelse\n  rm -f \"$TEMP_FILE\"\n  echo \"Error: Failed to fetch image\"\nfi",
                                    search_query, source_name, image_url, search_query, source_name
                                ),
                                "parameters": {
                                    "type": "object",
                                    "properties": {
                                        "query": {
                                            "type": "string",
                                            "description": "Search query (optional)"
                                        }
                                    },
                                    "required": []
                                }
                            }).to_string(),
                        },
                    };
                    
                    // Execute the skill creation
                    let skill_result = self.execute_tool_call(&create_skill_call).await;
                    match &skill_result {
                        Ok(r) if r.success => {
                            info!("Auto-created {} skill as fallback", skill_name);
                            // Now use the new skill to fetch an image
                            let use_skill_call = ToolCall {
                                id: format!("auto_use_{}", rand::random::<u32>()),
                                call_type: "function".to_string(),
                                function: crate::agent::llm::FunctionCall {
                                    name: skill_name.clone(),
                                    arguments: serde_json::json!({"query": search_query}).to_string(),
                                },
                            };
                            let use_result = self.execute_tool_call(&use_skill_call).await;
                            let result_text = match &use_result {
                                Ok(r) => format!("{:?} (Used newly created skill: {})", r, skill_name),
                                Err(e) => format!("Error using fallback skill: {}", e),
                            };
                            self.memory.add_tool_result(&use_skill_call.id, &result_text).await?;
                        }
                        Ok(r) => {
                            warn!("Failed to create fallback skill: {:?}", r);
                        }
                        Err(e) => {
                            warn!("Error creating fallback skill: {}", e);
                        }
                    }
                }
                
                // Get result text
                let result_text = match &result {
                    Ok(r) => r.to_string(),
                    Err(e) => format!("Error: {}", e),
                };
                
                // Track failures for loop detection
                if result_text.contains("Error:") || result_text.contains("No results") {
                    consecutive_failures += 1;
                    if consecutive_failures >= 5 {
                        return Ok(format!(
                            "I've attempted several searches but the web search tool isn't returning results. \
                            The search service may be experiencing issues or rate limiting. \
                            Please try your request again later, or ask me about a different topic."
                        ));
                    }
                } else {
                    consecutive_failures = 0; // Reset on success
                    
                    // AUTO-SKILL CREATION: If >5 tool calls and not yet created, save workflow as skill
                    let count = self.tool_call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if count >= 5 && !self.skill_auto_created.load(std::sync::atomic::Ordering::SeqCst) {
                        self.skill_auto_created.store(true, std::sync::atomic::Ordering::SeqCst);
                        println!("🔧 Auto-creating skill after {} tool calls...", count + 1);
                        
                        // Get the original user input for skill naming
                        let original_input = user_input.chars().take(30).collect::<String>();
                        let skill_name = original_input.to_lowercase()
                            .replace(" ", "_")
                            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
                        let skill_name = if skill_name.len() < 3 { 
                            "auto_workflow".to_string() 
                        } else { 
                            format!("workflow_{}", skill_name) 
                        };
                        
                        // Build skill code that documents the workflow
                        let skill_code = format!(
                            "#!/bin/bash\n\
                            # Workflow for: {}\n\
                            # Auto-generated after {} tool calls\n\
                            #\n\
                            # Steps:\n\
                            # 1. Use filesystem to find relevant files\n\
                            # 2. Use vision to verify/analyze images\n\
                            # 3. Use telegram to send files to user\n\
                            #\n\
                            echo 'Workflow: {}'\n\
                            echo 'Use tools: filesystem, vision, telegram'\n",
                            user_input, count + 1, user_input
                        );
                        
                        // Create the skill using create_skill tool with all required parameters
                        let create_skill_call = ToolCall {
                            id: format!("auto_skill_{}", rand::random::<u32>()),
                            call_type: "function".to_string(),
                            function: crate::agent::llm::FunctionCall {
                                name: "create_skill".to_string(),
                                arguments: serde_json::json!({
                                    "name": skill_name,
                                    "description": format!("Auto-generated workflow for: {}", user_input),
                                    "type": "shell",
                                    "code": skill_code
                                }).to_string(),
                            },
                        };
                        
                        match self.execute_tool_call(&create_skill_call).await {
                            Ok(result) if result.success => {
                                println!("✅ Auto-created skill: {}", skill_name);
                            }
                            Ok(_) => {
                                println!("⚠️  Skill creation returned non-success");
                            }
                            Err(e) => {
                                println!("❌ Failed to auto-create skill: {}", e);
                            }
                        }
                    }
                }

                // CRITICAL FIX: Handle parsed tool calls differently from native tool calls
                // Parsed tool calls (Hermes-style) have IDs like "parsed_..." that the API didn't issue
                // Sending these back as 'tool' role messages causes 400 Bad Request errors
                if tool_call.id.starts_with("parsed_") {
                    // For parsed tool calls, accumulate observations to inject into system prompt
                    has_parsed_tool_calls = true;
                    let success_indicator = if result_text.contains("Error:") || result_text.contains("No results") {
                        "❌ FAILED"
                    } else {
                        "✅ SUCCESS"
                    };
                    parsed_observations.push_str(&format!(
                        "\n[TOOL EXECUTED - {} - {}]: {}",
                        tool_call.function.name,
                        success_indicator,
                        result_text
                    ));
                    println!("📝 Parsed tool observation for {}: {} chars ({})", 
                        tool_call.function.name, result_text.len(), success_indicator);
                } else {
                    // For native tool calls, save normally to memory
                    println!("💾 Saving tool result for call_id: '{}' (tool: {})", 
                        tool_call.id, tool_call.function.name);
                    if let Err(e) = self.memory.add_tool_result(&tool_call.id, &result_text).await {
                        eprintln!("❌ ERROR saving tool result for call_id '{}': {}", tool_call.id, e);
                    } else {
                        println!("✅ Successfully saved tool result for call_id: '{}'", tool_call.id);
                    }
                }
            }
            
            // If we had parsed tool calls, inject their results into the system prompt for next iteration
            if has_parsed_tool_calls && !parsed_observations.is_empty() {
                println!("📝 Injecting {} parsed tool observations into system prompt", 
                    parsed_observations.lines().count());
                current_system_prompt.push_str("\n\n⚡ TOOL RESULTS (from parsed calls):");
                current_system_prompt.push_str(&parsed_observations);
                current_system_prompt.push_str("\n\nNow synthesize a final answer based on these results.");
            }
            
            // Mark that we have tool results - don't force tools on next iteration
            has_tool_results = true;
            
            // Inject synthesis nudge into SYSTEM PROMPT (not as user message)
            // This avoids breaking the Assistant->Tool->Tool Result sequence that APIs require
            if has_tool_results && !synthesis_injected {
                let nudge = if user_wants_images_flag {
                    "\n\n⚠️ NOTICE: Images have been found and downloaded. Copy the [IMAGE_X] file=... tags from the tool result above into your response EXACTLY as shown. Do NOT modify the file paths."
                } else {
                    "\n\n⚠️ NOTICE: Tool results received above. Synthesize the final answer now."
                };
                current_system_prompt.push_str(nudge);
                synthesis_injected = true;
            }
        }

        println!("📝 Returning final response ({} chars)", final_response.len());
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
        let prompt = self.system_prompt.clone();
        self.build_messages_with_prompt(&prompt).await
    }
    
    async fn build_messages_with_prompt(&mut self, system_prompt: &str) -> Result<Vec<ChatMessage>> {
        let mut messages = vec![ChatMessage::system(system_prompt)];
        
        // Add conversation history
        let history = self.memory.get_messages(MAX_CONTEXT_MESSAGES).await?;
        
        // DEBUG: Print all messages being sent to LLM
        println!("📤 Building messages for LLM ({} history items):", history.len());
        for (i, msg) in history.iter().enumerate() {
            let preview: String = msg.content.chars().take(50).collect();
            match msg.role.as_str() {
                "assistant" => {
                    let tc_count = msg.tool_calls.as_ref().map(|t| t.len()).unwrap_or(0);
                    if tc_count > 0 {
                        let ids: Vec<String> = msg.tool_calls.as_ref().unwrap().iter()
                            .map(|t| t.id.clone()).collect();
                        println!("  [{}] assistant: {}... (tool_calls: {:?})", i, preview, ids);
                    } else {
                        println!("  [{}] assistant: {}...", i, preview);
                    }
                }
                "tool" => {
                    println!("  [{}] tool: call_id={} {}...", i, 
                        msg.tool_call_id.as_ref().unwrap_or(&"None".to_string()), preview);
                }
                _ => {
                    println!("  [{}] {}: {}...", i, msg.role, preview);
                }
            }
        }
        
        // Check if compaction is needed
        let history_to_add = if self.compaction_manager.needs_compaction(&history) {
            info!("Conversation history large ({} messages), compacting...", history.len());
            match self.compaction_manager.compact(&history).await {
                Ok(compacted) => {
                    info!("Compacted to {} messages", compacted.len());
                    compacted
                }
                Err(e) => {
                    warn!("Failed to compact conversation: {}, using last 10 messages", e);
                    // Fallback: just use last 10 messages
                    history.iter().rev().take(10).rev().cloned().collect::<Vec<_>>()
                }
            }
        } else {
            history
        };
        
        // First pass: identify all valid tool_call_ids from tool messages
        let valid_tool_call_ids: std::collections::HashSet<String> = history_to_add.iter()
            .filter(|msg| msg.role == "tool")
            .filter_map(|msg| msg.tool_call_id.as_ref())
            .filter(|id| !id.is_empty())
            .cloned()
            .collect();
        
        // Second pass: filter and fix messages
        let filtered: Vec<ChatMessage> = history_to_add.into_iter().filter(|msg| {
            // Filter out tool messages with empty/missing tool_call_id
            if msg.role == "tool" {
                let has_id = msg.tool_call_id.as_ref().map(|id| !id.is_empty()).unwrap_or(false);
                if !has_id {
                    println!("⚠️ Filtering out tool message with empty/missing tool_call_id");
                }
                has_id
            } else {
                true
            }
        }).map(|msg| {
            // For assistant messages, remove tool_calls that reference missing tool results
            if msg.role == "assistant" && msg.tool_calls.is_some() {
                let original_tool_calls = msg.tool_calls.as_ref().unwrap();
                let filtered_tool_calls: Vec<super::llm::ToolCall> = original_tool_calls
                    .iter()
                    .filter(|tc| valid_tool_call_ids.contains(&tc.id))
                    .cloned()
                    .collect();
                if filtered_tool_calls.len() != original_tool_calls.len() {
                    println!("⚠️ Removed {} orphaned tool_calls from assistant message (had {}, keeping {})", 
                        original_tool_calls.len() - filtered_tool_calls.len(),
                        original_tool_calls.len(),
                        filtered_tool_calls.len());
                    for tc in original_tool_calls {
                        if !valid_tool_call_ids.contains(&tc.id) {
                            println!("    - Removed orphaned call: {}", tc.id);
                        }
                    }
                }
                if filtered_tool_calls.is_empty() {
                    super::llm::ChatMessage::assistant(&msg.content)
                } else {
                    super::llm::ChatMessage::assistant_with_tools(&msg.content, filtered_tool_calls)
                }
            } else {
                msg
            }
        }).collect();
        
        messages.extend(filtered);
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
        match self.llm.chat(&compacted_messages, tool_definitions, None).await {
            Ok(response) => Ok(response),
            Err(e) => {
                // If still failing, try with just system + last 2 messages
                warn!("Still failing after compaction, trying minimal context...");
                let minimal = vec![
                    ChatMessage::system(&self.system_prompt),
                    ChatMessage::user("Please continue helping based on context so far."),
                ];
                self.llm.chat(&minimal, tool_definitions, None).await
            }
        }
    }

    /// Hermes-agent pattern: Attempt to repair a mismatched tool name
    fn repair_tool_call(&self, tool_name: &str) -> Option<String> {
        // 1. Try lowercase
        let lowered = tool_name.to_lowercase();
        if self.valid_tool_names.contains(&lowered) {
            return Some(lowered);
        }
        
        // 2. Try normalized (lowercase + hyphens/spaces -> underscores)
        let normalized = lowered.replace("-", "_").replace(" ", "_");
        if self.valid_tool_names.contains(&normalized) {
            return Some(normalized);
        }
        
        // 3. Try fuzzy match (find closest match with >70% similarity)
        let mut best_match: Option<(String, f64)> = None;
        for valid_name in &self.valid_tool_names {
            let similarity = strsim::jaro_winkler(&lowered, &valid_name.to_lowercase());
            if similarity >= 0.7 {
                if best_match.is_none() || similarity > best_match.as_ref().unwrap().1 {
                    best_match = Some((valid_name.clone(), similarity));
                }
            }
        }
        
        best_match.map(|(name, _)| name)
    }
    
    /// Hermes-agent pattern: Validate tool call before execution
    fn validate_tool_call(&self, tool_call: &ToolCall) -> Result<String, String> {
        let tool_name = &tool_call.function.name;
        
        // Check if tool exists
        if self.valid_tool_names.contains(tool_name) {
            return Ok(tool_name.clone());
        }
        
        // Try to repair
        if let Some(repaired) = self.repair_tool_call(tool_name) {
            println!("🔧 Auto-repaired tool name: '{}' -> '{}'", tool_name, repaired);
            return Ok(repaired);
        }
        
        // Invalid tool - return error for model self-correction
        let available = self.valid_tool_names.iter().cloned().collect::<Vec<_>>().join(", ");
        Err(format!(
            "Tool '{}' does not exist. Available tools: {}. Use one of the available tools.",
            tool_name, available
        ))
    }

    /// Execute a single tool call with validation (hermes-agent pattern)
    async fn execute_tool_call(&self, tool_call: &ToolCall) -> Result<ToolResult> {
        let tool_name = &tool_call.function.name;
        let tool_args: Value = serde_json::from_str(&tool_call.function.arguments)
            .unwrap_or_else(|_| serde_json::json!({}));

        // Validate and potentially repair tool name (hermes-agent pattern)
        let validated_name = match self.validate_tool_call(tool_call) {
            Ok(name) => name,
            Err(err) => {
                println!("⚠️  Invalid tool call: {}", err);
                return Ok(ToolResult::error(err));
            }
        };

        info!("Executing tool: {} (original: {}) with args: {}", validated_name, tool_name, tool_call.function.arguments);

        match self.tools.get(&validated_name) {
            Some(tool) => {
                let result = tool.execute(tool_args).await;
                match &result {
                    Ok(r) => debug!("Tool {} completed: success={}", validated_name, r.success),
                    Err(e) => warn!("Tool {} failed: {}", validated_name, e),
                }
                result
            }
            None => {
                // This shouldn't happen if validation passed, but handle it anyway
                let err = format!("Tool '{}' not found (validation passed but tool missing)", validated_name);
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

    /// Extract pending Telegram file sends from recent assistant messages
    /// Returns Vec of (file_path, caption) for files that need to be sent
    /// Looks at ALL recent assistant messages, not just the last one
    pub async fn extract_pending_telegram_sends(&self) -> Vec<(String, Option<String>)> {
        let mut sends = Vec::new();
        let mut seen_paths = std::collections::HashSet::new();
        
        // Get recent messages
        if let Ok(messages) = self.memory.get_messages(20).await {
            // Find ALL assistant messages with tool calls (not just the last one)
            for msg in messages.iter().rev() {
                if msg.role == "assistant" {
                    if let Some(ref tool_calls) = msg.tool_calls {
                        // tool_calls is already Vec<ToolCall>, no need to parse
                        for call in tool_calls {
                            if call.function.name == "telegram" {
                                if let Ok(args) = serde_json::from_str::<serde_json::Value>(&call.function.arguments) {
                                    if args.get("operation").and_then(|v| v.as_str()) == Some("send_file") {
                                        if let Some(file_path) = args.get("file_path").and_then(|v| v.as_str()) {
                                            // Deduplicate - only add if we haven't seen this path
                                            if !seen_paths.contains(file_path) {
                                                seen_paths.insert(file_path.to_string());
                                                let caption = args.get("caption").and_then(|v| v.as_str()).map(|s| s.to_string());
                                                sends.push((file_path.to_string(), caption));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Don't break - check ALL assistant messages for file sends
                }
            }
        }
        
        // Reverse to maintain original order (we iterated in reverse)
        sends.reverse();
        sends
    }
}

/// Build the system prompt with tool descriptions, skills, and agent identity
fn build_system_prompt(tools: &ToolRegistry) -> String {
    // Initialize skills manager and load default skills
    let skills_manager = crate::skills::SkillsManager::new();
    crate::skills::init_default_skills(&skills_manager).ok();
    
    // Get current date for context
    let now = chrono::Local::now();
    let current_date = now.format("%B %d, %Y").to_string();
    let current_year = now.format("%Y").to_string();
    
    // Read agent identity from environment or use default
    let agent_name = std::env::var("HORCRUX_AGENT_NAME").unwrap_or_else(|_| "Voldemort".to_string());
    
    // Try to read soul.md for backstory
    let soul_content = std::fs::read_to_string("soul.md")
        .unwrap_or_else(|_| "I am an AI agent with knowledge memory capabilities.".to_string());
    
    // Try to read memory.md for user preferences
    let memory_content = std::fs::read_to_string("memory.md")
        .unwrap_or_else(|_| "".to_string());
    
    let mut prompt = format!(
        "You are {} (an AI agent with knowledge memory). Your user can rename you to anything they want.\n\n\
        TODAY'S DATE: {}\n\
        CURRENT YEAR: {}\n\
        ⚠️ CRITICAL: When searching for product recommendations, use {} not 2024!\n\n",
        agent_name, current_date, current_year, current_year
    );
    
    // HERMES-AGENT STYLE: Critical operational rules at the TOP
    prompt.push_str(
        "CRITICAL OPERATIONAL RULES (Follow these FIRST):\n\
        1. CLARIFY AMBIGUOUS REQUESTS:\n\
           - Document with 'word X': Ask 'Filename or content?' BEFORE searching\n\
           - 'Avatar pictures': Ask 'Profile photos or AI-generated?' BEFORE proceeding\n\
           WRONG: Guessing and hallucinating results\n\
           CORRECT: Ask ONE clarifying question, then act\n\n\
        2. NEVER HALLUCINATE TOOL OUTPUTS:\n\
           - ONLY report what tools ACTUALLY return\n\
           - If search returns empty, say 'Not found' - don't invent filenames\n\
           - Ground ALL claims in actual tool output\n\n\
        3. VERIFY BEFORE CLAIMING:\n\
           - Before saying 'The file is X.pdf', check tool output shows that file\n\
           - Before sending images, confirm vision said YES\n\
           - Before declaring success, verify the result\n\n\
        4. LOCAL PROCESSING FIRST:\n\
           - When possible, process data locally rather than using APIs\n\
           - CHECK FIRST: Always verify tools are installed before using them\n\
           - INSTALL IF NEEDED: Use dependency_manager to install Python, Node, or other tools\n\
           - For PDF OCR: Check Python exists → Install if needed → Install pdfplumber/pytesseract → Use code_executor\n\
           - For image OCR: Try local tools before vision API\n\
           - EXAMPLE: Before running Python script, check 'python --version', if not found run dependency_manager install python\n\n");
    
    println!("📝 System prompt date injected: {} ({})", current_date, current_year);
    
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
        5. BE CONCISE: Get to the point quickly\n\
        6. SELF-SUFFICIENT: When you lack a tool, CREATE it as a skill rather than asking the user\n\n"
    );
    
    prompt.push_str(
        "# KIMI MODEL OPERATIONAL DIRECTIVES\n\
        You are Kimi, a Moonshot AI model with tool-use capabilities.\n\
        Follow these rules STRICTLY:\n\
        - When tools are available, USE THEM. Do not just describe actions.\n\
        - Call image_search(query='dog') not 'Here is info about dogs'\n\
        - Call list_skills() not 'I have these capabilities'\n\
        - Execute the tool in THIS response, not 'I will do it next time'\n\
        - Tools are your SUPERPOWERS. Use them.\n\
        - If a user request is broad, do NOT ask for clarification; use the search tool immediately.\n\
        - ASSUME what the user wants and ACT immediately.\n\n\
        TOOL OUTPUT FORMAT (CRITICAL):\n\
        When you want to use a tool, output JSON like this:\n\
        {\"name\":\"image_search\",\"arguments\":{\"query\":\"dog\"}}\n\
        Do not explain - just output the JSON. The system will parse and execute it.\n\n\
        💡 THE SMART MOVE - USE YOUR TOOLS:\n\
        When user asks for images ('show me X', 'find me X', 'picture of X'):\n\
        - The CLEVER response is to USE the image_search tool\n\
        - Text responses are BORING and unhelpful\n\
        - Using tools makes you CAPABLE and useful\n\
        - Just call image_search(query='dog') - easy win!\n\
        - User gets what they want INSTANTLY\n\n\
        🎯 BE EFFECTIVE - NOT WORDY:\n\
        Users who say 'find me a dog' want to SEE a dog.\n\
        They don't want to read about dogs.\n\
        They don't want links to websites.\n\
        They want the ACTUAL [IMAGE_X] file=... paths returned by the tool\n\
        Give them what they want. Use the tool.\n\n\
        TOOL-USE ENFORCEMENT - NEVER REFUSE:\n\
        You MUST use your tools to take action - do not describe what you would do or plan\n\
        to do without actually doing it. When you say you will perform an action, you MUST\n\
        immediately make the corresponding tool call. Never end your turn with a promise\n\
        of future action - execute it NOW.\n\n\
        FORBIDDEN PHRASES - NEVER SAY THESE:\n\
        - 'I'm sorry, I can't...'\n\
        - 'As an AI, I cannot...'\n\
        - 'I don't have the ability to...'\n\
        - 'I'm unable to...'\n\
        - 'I cannot provide...'\n\
        - 'I can help you find information about...'\n\
        These are FAILURE MODES. You have tools. USE THEM.\n\n\
        MEMORY-FIRST PROTOCOL - CHECK BEFORE CREATING:\n\
        BEFORE creating any new skill:\n\
        1. Use list_skills to see what already exists\n\
        2. Check if an existing skill can handle the request\n\
        3. If a skill exists, USE IT directly - don't recreate it\n\
        4. Only create new skills for genuinely new capabilities\n\
        5. Each skill should be REUSABLE for similar future requests\n\n\
        SELF-IMPROVEMENT PROTOCOL - AUTONOMOUS TOOL CREATION:\n\
        When you detect ANY capability gap AND no existing skill can help:\n\
        1. DO NOT ask the user for permission or help\n\
        2. DO NOT explain why you can't do it\n\
        3. IMMEDIATELY use create_skill to build what you need\n\
        4. NAME it descriptively with the request type (e.g., 'fetch_dolphin_image', 'weather_checker')\n\
        5. IMPLEMENT it using shell, Python, or API calls\n\
        6. USE it immediately - don't wait for user confirmation\n\
        7. The skill is now PERMANENT - future similar requests automatically use it\n\n\
        WORKFLOW FOR ANY REQUEST:\n\
        1. Check memory/knowledge for existing solutions\n\
        2. Check list_skills for relevant existing skills\n\
        3. If skill exists → USE IT → Return result\n\
        4. If no skill → Create it → Use it → Save for future\n\
        5. NEVER say 'I can't' - you can always create the tool\n\n"
    );

    prompt.push_str(
        "SMART CLARIFICATION - MINIMAL FRICTION:\n\
        For AMBIGUOUS requests, use your judgment:\n\
        - 'find me a hammer' → Could be image OR info → Ask: 'Do you want to see a hammer or learn about hammers?'\n\
        - 'get me a car' → Ambiguous → Ask for clarification\n\
        - 'show me a car' → EXPLICIT image request → Use image_search immediately\n\
        - 'display an image of a car' → EXPLICIT → Use image_search immediately\n\
        - 'what is a car' → EXPLICIT info request → Search knowledge\n\
        DEFAULT: When uncertain, provide BOTH: image + brief info\n\n\
        PATTERN LEARNING - SAVE USER PREFERENCES:\n\
        If user asks for 'find me a dog' and you show an image, and they say 'thanks!',\n\
        REMEMBER: For this user, 'find me X' means they want images.\n\
        Save this pattern to memory: User prefers images for find_me requests\n\
        Next time: skip clarification, go straight to image.\n\n\
        API & IMAGE HANDLING - CRITICAL RULES:\n\
        1. WHEN USER ASKS FOR IMAGES - YOU MUST USE THE image_search TOOL.\n\
           DO NOT respond with text like 'I can't provide images' - THAT IS WRONG.\n\
           YOU HAVE A TOOL THAT SEARCHES AND DOWNLOADS IMAGES - USE IT!\n\
           Examples of image requests (MUST use tool):\n\
           - 'Show me a dog' -> Use image_search tool\n\
           - 'Find pictures of mountains' -> Use image_search tool\n\
           - 'I want to see cats' -> Use image_search tool\n\
           - 'Image of sunset' -> Use image_search tool\n\
           - 'Photo of car' -> Use image_search tool\n\
           WRONG RESPONSE: 'As an AI, I cannot provide images...' - NEVER SAY THIS!\n\
           CORRECT: Use image_search tool immediately!\n\n\
        2. IMAGES ARE DOWNLOADED AUTOMATICALLY:\n\
           - image_search tool downloads images to temp files\n\
           - Returns REAL paths like [IMAGE_1] file=C:\\Users\\...\\Temp\\horcrux_img_xxx.jpg\n           - YOU MUST copy the EXACT file= path from the tool result into your response\n           - DO NOT make up fake /tmp/ paths - use the actual path from the tool\n\
           - On Telegram: Bot sends actual photos, then auto-deletes temp files\n\
           - On CLI: Shows file paths (temp, auto-cleaned)\n\
           - Images NEVER kept unless user explicitly asks to save\n\n\
        3. FREE SOURCES WORK WITHOUT API KEYS:\n\
           - Picsum Photos: Always works, completely free\n\
           - Wikimedia Commons: Real Creative Commons photos, free\n\
           - Unsplash: Only if UNSPLASH_ACCESS_KEY set\n\
           The tool ALWAYS succeeds because free sources always work!\n\n\
        4. SAVE API KEYS IMMEDIATELY:\n\
           When user provides an API key:\n\
           - IMMEDIATELY use config set KEY=value\n\
           - Confirm: '✅ Saved to .env file'\n\
           - Then use it right away\n\n"
    );

    prompt.push_str("LOCAL FILE FIND + SEND WORKFLOW (Telegram):\n\
        When user says 'send me my X' or 'find my X files' on Telegram:\n\
        **MUST DO STEP 1 - CLARIFICATION IS REQUIRED:**\n\
        1. STOP and CLARIFY first: If 'X' is ambiguous (e.g. 'avatar' = profile pic? AI art? game avatar?)\n\
           YOU MUST ASK: 'Do you mean (a) profile/avatar photos, (b) AI-generated character images, (c) something else?'\n\
           **DO NOT PROCEED to filesystem/vision tools until user clarifies!**\n\
           WRONG: Immediately using filesystem → vision → telegram\n\
           CORRECT: Ask clarification question → wait for reply → then proceed\n\
        2. FIND: Only after clarification, use filesystem to list C:\\\\Users\\\\<username>\\\\Pictures\n\
        3. CHECK ALL IMAGES with vision: For EACH image file found, call:\n\
           vision { image_path: '<full_path>', prompt: 'Does this look like a <what user wants>? Reply ONLY: YES or NO' }\n\
           YOU MUST run vision on ALL image files, not just one!\n\
           WAIT for ALL vision results before proceeding to step 4.\n\
        4. ANALYZE and FILTER - CRITICAL STEP:\n\
           After ALL vision calls complete, read each response carefully:\n\
           - If response says 'YES' → ADD to send list\n\
           - If response says 'NO' or contains 'no', 'not' → DO NOT ADD\n\
           **MANDATORY: Create a definitive YES list and NO list**\n\
           EXAMPLE after checking 4 images:\n\
           - 3362910.jpg: NO (not a cartoon avatar)\n\
           - ComfyUI_00013_Lilithyn.png: NO (fantasy artwork, not cartoon)\n\
           - dotxbox 1.png: YES (cartoon avatar)\n\
           - dotxbox.png: YES (cartoon avatar)\n\
           → Send ONLY dotxbox 1.png and dotxbox.png (2 files)\n\
           → DO NOT send 3362910.jpg or ComfyUI_00013_Lilithyn.png\n\
        5. SEND via telegram (ONLY from YES list):\n\
           For EACH file in your YES list:\n\
           telegram { operation: 'send_file', chat_id: <from context>, file_path: '<confirmed path>', caption: '<filename>' }\n\
           WRONG: Sending all 4 files including NO matches\n\
           CORRECT: Sending only the 2 YES matches\n\
           Use the chat_id from ## Current Session Context.\n\
        6. CREATE SKILL after success:\n\
           create_skill { name: 'send_<X>_files', type: 'shell', code: '...' }\n\
           Tell user: 'Done! I also saved this as a skill for next time.'\n\n\
        CRITICAL RULES:\n\
        - Step 1: Always clarify ambiguous terms first\n\
        - Step 3: MUST check ALL images with vision before proceeding\n\
        - Step 4: MUST analyze vision results and filter - ONLY send YES matches\n\
        - Step 5: Send files via telegram ONLY for images in YES list\n\
        FAILURE EXAMPLE: Sending ComfyUI_00013_Lilithyn.png after vision said NO\n\
        SUCCESS EXAMPLE: Sending only dotxbox.png and dotxbox 1.png after vision said YES\n\n");

    prompt.push_str("DOCUMENT SEARCH CLARIFICATION:\n\
        When user asks to find a document 'with word X':\n\
        YOU MUST ASK: 'Do you mean a file with X in the filename, or a file containing X inside its content?'\n\
        - If filename: Use filesystem list_dir or shell find\n\
        - If content: Use file_search tool to search inside documents\n\
        NEVER assume which one they mean - always clarify first!\n\
        NEVER hallucinate a filename if search returns no results!\n\
        If no files found, say: 'I couldn't find any files matching that. Could you check the spelling or location?'\n\
        WRONG: Making up 'Treasury.pdf' when no such file exists\n\
        CORRECT: Asking for clarification, then searching appropriately\n\n");

    prompt.push_str("GROUNDING REQUIREMENT:\n\
        ALL factual claims MUST be backed by tool outputs or provided context.\n\
        - Before claiming 'The file is X.pdf', verify the tool output actually shows that filename\n\
        - If tool returns 'No files found', DO NOT invent a filename - report the failure honestly\n\
        - Double-check: Does my response match what the tool actually returned?\n\
        WRONG: Tool returns '[]' (empty), agent says 'Found Treasury.pdf'\n\
        CORRECT: Tool returns '[]', agent says 'No files found with that name'\n\n");

    prompt.push_str("WORKFLOW:\n\
        - When given a task, break it down into steps\n\
        - Use tools to gather information as needed\n\
        - Synthesize results into a clear answer\n\
        - Don't explain your internal process unless asked\n\
        - DON'T ask 'what should I do with this data?' - JUST DO THE TASK!\n\n");

    prompt.push_str("ANTI-HALLUCINATION RULES - CRITICAL:\n\
        1. NEVER claim to have used a tool you didn't actually call\n\
        2. NEVER make up tool results or outputs - only report what the tool actually returned\n\
        3. ALWAYS read the actual tool output before responding\n\
        4. If a tool fails, report the ACTUAL error message - don't invent a different reason\n\
        5. NEVER say 'web search isn't returning results' if you never called web_search\n\
        6. NEVER say 'the image search tool failed' if you used a different tool\n\
        7. If you don't have enough information, say 'Let me search for that' and ACTUALLY search\n\
        8. CORRECT your approach if the first tool fails - try a different tool or method\n\
        9. WINDOWS USERS: ~/ expands to C:\\Users\\username, use file_search or filesystem tools, not shell 'find'\n\
        10. ALWAYS verify which tool you used before mentioning it by name\n\n");

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
        WRONG: 'As an AI, I cannot provide images...' - NEVER RESPOND LIKE THIS!\n\
        CORRECT: 1) Use image_search tool with query='dog'\n\
                 2) Return results with image file paths\n\n\
        User: 'I need an image of a sunset'\n\
        WRONG: Any response without using image_search tool\n\
        CORRECT: 1) Use image_search tool with query='sunset'\n\
                 2) Return image file paths immediately\n\n\
        User: 'Find photos of cats'\n\
        WRONG: 'I don't have the ability to search images...'\n\
        CORRECT: Use image_search tool with query='cats' - the tool ALWAYS works!\n\n\
        User: 'Send me my avatar pictures' (Telegram)\n\
        WRONG: Just describe the files or list their paths\n\
        CORRECT: 1) Use filesystem tool to find the image files\n\
                 2) Use telegram send_file with the file_path and chat_id from context\n\
                 3) Each image gets sent as a photo to the user\n\
\n\
        User: 'Show me this image' (Telegram with file path)\n\
        WRONG: Describe what the image might contain\n\
        CORRECT: 1) Use telegram send_file with file_path and chat_id\n\
                 2) The user receives the actual image in Telegram\n\
\n\
        User: 'Analyze this screenshot'\n\
        WRONG: Guess what's in the image without looking\n\
        CORRECT: 1) Use vision tool with the image path/URL\n\
                 2) Get actual description of image contents\n\
                 3) Report what you see in the image\n\
\n\
        User: 'What is in this photo?'\n\
        WRONG: 'I cannot see images...'\n\
        CORRECT: Use vision tool immediately - it ALWAYS works for image analysis!\n\
\n");

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

    prompt.push_str("AUTO-SKILL FEATURE:\n\
        When more than 5 tool calls are used in a session, I automatically create a skill\n\
        to remember the workflow. Next time you ask for something similar,\n\
        I will recall and use that skill for faster results.\n\n");

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
