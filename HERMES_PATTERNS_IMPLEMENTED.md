# Hermes-Agent Patterns Implemented

This document tracks the architectural patterns from hermes-agent that have been adapted to our Rust implementation.

## ✅ Implemented

### 1. Tool Name Validation & Repair
**File**: `src/agent/react.rs`

**Pattern**: Validate tool calls against a set of valid tool names, with auto-repair for common mismatches.

**Implementation**:
```rust
pub struct ReActAgent {
    // ...
    valid_tool_names: std::collections::HashSet<String>,
}

fn repair_tool_call(&self, tool_name: &str) -> Option<String> {
    // 1. Try lowercase
    // 2. Try normalized (hyphens/spaces -> underscores)
    // 3. Try fuzzy match with Jaro-Winkler similarity > 0.7
}

fn validate_tool_call(&self, tool_call: &ToolCall) -> Result<String, String> {
    // Returns Ok(repaired_name) or Err(error_message_to_model)
}
```

**Benefits**:
- Prevents hallucinated tool names
- Auto-fixes case mismatches (WebSearch → web_search)
- Model self-corrects when given clear error messages

---

### 2. Tool Pair Preservation
**File**: `src/agent/react.rs` - `build_messages_with_prompt()`

**Pattern**: After context compression, ensure tool_call/tool_result pairs stay synchronized.

**Implementation**:
```rust
// First pass: identify valid tool_call_ids
let valid_tool_call_ids: HashSet<String> = ...;

// Second pass: filter orphaned tool messages and assistant tool_calls
let filtered = history.into_iter().filter(|msg| {
    // Remove tool messages with empty IDs
    // Remove assistant tool_calls that reference missing tool results
}).collect();
```

**Benefits**:
- Prevents "tool_call_id not found" API errors
- Maintains conversation integrity after compaction
- Clean history for the model

---

### 3. Anti-Hallucination System Prompt Rules
**File**: `src/agent/react.rs` - `build_system_prompt()`

**Pattern**: Explicit instructions to prevent claiming tool usage that didn't happen.

**Added Rules**:
```
ANTI-HALLUCINATION RULES - CRITICAL:
1. NEVER claim to have used a tool you didn't actually call
2. NEVER make up tool results or outputs
3. ALWAYS read the actual tool output before responding
4. If a tool fails, report the ACTUAL error message
5. NEVER say 'web search isn't returning results' if you never called web_search
```

---

### 4. Cross-Platform Path Handling
**File**: `src/tools/filesystem.rs`, `src/tools/shell.rs`

**Pattern**: Automatic path expansion and platform detection.

**Implementation**:
```rust
fn expand_path(&self, path: &str) -> PathBuf {
    if path.starts_with("~/") {
        dirs::home_dir().join(&path[2..])
    } else {
        PathBuf::from(path)
    }
}

fn get_error_context(&self, path: &str, err: &std::io::Error) -> String {
    // Platform-specific hints
    if cfg!(windows) && path.contains('/') {
        "💡 Hint: You're using Unix-style paths on Windows..."
    }
}
```

---

### 5. Dependency Manager (Self-Sufficiency)
**File**: `src/tools/dependency_manager.rs`

**Pattern**: Agent can install its own dependencies when needed.

**Features**:
- Detects available package managers (winget, brew, apt)
- Installs languages: Python, Node.js, Rust
- Installs packages: pip, npm
- Requires explicit user consent

---

### 6. Code Executor (Local Execution)
**File**: `src/tools/code_executor.rs`

**Pattern**: Execute code locally without API calls.

**Features**:
- Python, Node.js, Rust, Shell execution
- Timeout protection
- Automatic cleanup of temp files
- Suggests installing missing languages

---

## 🔄 Partially Implemented / Future Work

### 7. Parallel Tool Execution
**Status**: Framework exists but not enabled

**Hermes Pattern**:
- Read-only tools can run concurrently
- Path-scoped tools can run concurrently if paths don't overlap
- Interactive tools must be sequential

**Implementation Sketch**:
```rust
pub enum ToolConcurrency {
    Never,      // Interactive tools
    Always,     // Read-only safe
    PathScoped, // File operations
}
```

---

### 8. Structured Error Classification
**Status**: Not implemented

**Hermes Pattern**:
```python
class FailoverReason(enum.Enum):
    auth = "auth"
    rate_limit = "rate_limit"
    context_overflow = "context_overflow"
    # ...
```

**Benefits**: Different recovery strategies for different error types.

---

### 9. Tiered Context Compression
**Status**: Basic compaction exists, could be enhanced

**Hermes Pattern**:
- Protect first N messages (system + initial context)
- Protect last N messages (recent history)
- Summarize middle section iteratively
- Iterative summary updates (preserve previous summary context)

---

### 10. Message Sanitization Pipeline
**Status**: Basic filtering exists

**Hermes Pattern**:
Multiple sanitization passes before API calls:
1. Budget warning stripping
2. Orphan tool pair cleanup
3. Role validation
4. Provider-specific field stripping

---

### 11. Toolsets (Composable Tool Groups)
**Status**: Not implemented

**Hermes Pattern**:
```python
TOOLSETS = {
    "web": {
        "tools": ["web_search", "web_extract"],
        "includes": ["http"]
    },
    "debugging": {
        "tools": ["terminal"],
        "includes": ["web", "file"]
    }
}
```

---

### 12. Structured Logging with Redaction
**Status**: Basic tracing exists

**Hermes Pattern**:
```python
class RedactingFormatter(logging.Formatter):
    SENSITIVE_PATTERNS = [
        (re.compile(r'(api[_-]?key["\s]*[:=]\s*)["\']?[\w-]+["\']?', re.I), r'\1***'),
    ]
```

---

### 13. Session Persistence
**Status**: SQLite storage exists, could be enhanced

**Hermes Pattern**:
- JSONL trajectory logs for debugging
- SQLite for session search and continuation
- Automatic persistence on any exit path

---

### 14. Markdown-Based Skills
**Status**: Basic skills exist, could enhance format

**Hermes Pattern**:
```markdown
---
name: rust-error-handling
platforms: [macos, linux]
metadata:
  hermes:
    fallback_for_toolsets: ["file"]
---

# Skill content...
```

---

## 📊 Comparison Summary

| Pattern | Hermes (Python) | Horcrux (Rust) | Status |
|---------|-----------------|----------------|--------|
| Tool Validation | ✅ | ✅ | Implemented |
| Tool Repair | ✅ | ✅ | Implemented |
| Pair Preservation | ✅ | ✅ | Implemented |
| Parallel Execution | ✅ | ⚠️ | Framework only |
| Error Classification | ✅ | ❌ | Not implemented |
| Tiered Compression | ✅ | ⚠️ | Basic only |
| Sanitization Pipeline | ✅ | ⚠️ | Basic only |
| Toolsets | ✅ | ❌ | Not implemented |
| Redacted Logging | ✅ | ⚠️ | Basic tracing |
| Session Persistence | ✅ | ✅ | SQLite works |
| Skills System | ✅ | ⚠️ | Simpler format |

---

## 🎯 Key Wins from Hermes Patterns

1. **No more tool_call_id errors** - Tool pair preservation fixed the API errors
2. **No more hallucinated tool names** - Validation + repair catches mismatches
3. **Better Windows support** - Path expansion and platform hints
4. **Self-sufficient agent** - Can install its own dependencies
5. **Local code execution** - No API needed for simple tasks

## 🚀 Next Priority Improvements

1. **Parallel tool execution** - Big perf win for read-only operations
2. **Error classification** - Better recovery from different failure modes
3. **Toolsets** - Organize tools into logical groups
4. **Structured logging** - Better observability and debugging
