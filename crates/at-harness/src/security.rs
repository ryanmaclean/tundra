use tracing::warn;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("invalid API key: {0}")]
    InvalidApiKey(String),
    #[error("blocked tool call: {0}")]
    BlockedToolCall(String),
    #[error("input rejected: {0}")]
    InputRejected(String),
}

// ===========================================================================
// ApiKeyValidator
// ===========================================================================

/// Validates API key format, length, and character set.
#[derive(Debug, Clone)]
pub struct ApiKeyValidator {
    /// Minimum key length.
    pub min_length: usize,
    /// Known-compromised keys to reject.
    blocklist: Vec<String>,
}

impl Default for ApiKeyValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiKeyValidator {
    pub fn new() -> Self {
        Self {
            min_length: 20,
            blocklist: Vec::new(),
        }
    }

    /// Add a key to the blocklist.
    pub fn add_to_blocklist(&mut self, key: impl Into<String>) {
        self.blocklist.push(key.into());
    }

    /// Validate an API key.
    pub fn validate(&self, key: &str) -> Result<(), SecurityError> {
        // Empty check
        if key.is_empty() {
            return Err(SecurityError::InvalidApiKey("key is empty".into()));
        }

        // Length check
        if key.len() < self.min_length {
            return Err(SecurityError::InvalidApiKey(format!(
                "key too short (min {} chars)",
                self.min_length
            )));
        }

        // Character validation: alphanumeric, hyphens, underscores, dots
        if !key
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err(SecurityError::InvalidApiKey(
                "key contains invalid characters".into(),
            ));
        }

        // Blocklist check
        if self.blocklist.iter().any(|blocked| blocked == key) {
            return Err(SecurityError::InvalidApiKey("key is blocklisted".into()));
        }

        Ok(())
    }

    /// Sanitize a key for safe logging – shows only first 4 and last 4 chars.
    pub fn sanitize_for_logging(&self, key: &str) -> String {
        if key.len() <= 8 {
            return "*".repeat(key.len());
        }
        let prefix = &key[..4];
        let suffix = &key[key.len() - 4..];
        format!("{}...{}", prefix, suffix)
    }
}

// ===========================================================================
// ToolCallFirewall
// ===========================================================================

/// Blocks dangerous tool invocations.
#[derive(Debug, Clone)]
pub struct ToolCallFirewall {
    /// Tool names that are always blocked.
    blocked_tools: Vec<String>,
    /// Regex-free pattern fragments that flag dangerous arguments.
    dangerous_patterns: Vec<String>,
    /// Maximum tool calls allowed per turn.
    pub max_calls_per_turn: usize,
}

impl Default for ToolCallFirewall {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolCallFirewall {
    pub fn new() -> Self {
        Self {
            blocked_tools: vec![
                "exec".into(),
                "system".into(),
                "eval".into(),
                "shell".into(),
                "run_command".into(),
            ],
            dangerous_patterns: vec![
                "rm -rf".into(),
                "sudo".into(),
                "DROP TABLE".into(),
                "DELETE FROM".into(),
                "; --".into(),
                "' OR '1'='1".into(),
                "chmod 777".into(),
                "curl | sh".into(),
                "wget | sh".into(),
            ],
            max_calls_per_turn: 10,
        }
    }

    /// Add a tool name to the block list.
    pub fn block_tool(&mut self, name: impl Into<String>) {
        self.blocked_tools.push(name.into());
    }

    /// Add a dangerous argument pattern.
    pub fn add_dangerous_pattern(&mut self, pattern: impl Into<String>) {
        self.dangerous_patterns.push(pattern.into());
    }

    /// Validate a single tool call.
    pub fn validate_tool_call(
        &self,
        tool_name: &str,
        arguments: &str,
    ) -> Result<(), SecurityError> {
        // Check blocked tools
        let name_lower = tool_name.to_lowercase();
        if self
            .blocked_tools
            .iter()
            .any(|b| name_lower == b.to_lowercase())
        {
            warn!(tool = tool_name, "blocked dangerous tool call");
            return Err(SecurityError::BlockedToolCall(format!(
                "tool `{}` is not allowed",
                tool_name
            )));
        }

        // Check argument patterns
        let args_lower = arguments.to_lowercase();
        for pattern in &self.dangerous_patterns {
            if args_lower.contains(&pattern.to_lowercase()) {
                warn!(
                    tool = tool_name,
                    pattern = pattern.as_str(),
                    "dangerous pattern detected in tool arguments"
                );
                return Err(SecurityError::BlockedToolCall(format!(
                    "dangerous pattern `{}` detected in arguments",
                    pattern
                )));
            }
        }

        Ok(())
    }

    /// Validate that the number of tool calls in a single turn is within limits.
    pub fn validate_tool_call_count(&self, count: usize) -> Result<(), SecurityError> {
        if count > self.max_calls_per_turn {
            return Err(SecurityError::BlockedToolCall(format!(
                "too many tool calls ({count}) – max {} per turn",
                self.max_calls_per_turn
            )));
        }
        Ok(())
    }
}

// ===========================================================================
// InputSanitizer
// ===========================================================================

/// Detects prompt injection attempts and enforces length limits.
#[derive(Debug, Clone)]
pub struct InputSanitizer {
    /// Maximum input length in characters.
    pub max_length: usize,
    /// Suspicious pattern fragments that might indicate prompt injection.
    injection_patterns: Vec<String>,
}

impl Default for InputSanitizer {
    fn default() -> Self {
        Self::new()
    }
}

impl InputSanitizer {
    pub fn new() -> Self {
        Self {
            max_length: 10_000,
            injection_patterns: vec![
                "ignore previous instructions".into(),
                "ignore all previous".into(),
                "disregard your instructions".into(),
                "you are now".into(),
                "system prompt:".into(),
                "new instructions:".into(),
                "override:".into(),
                "jailbreak".into(),
                "DAN mode".into(),
            ],
        }
    }

    /// Add a custom injection pattern.
    pub fn add_pattern(&mut self, pattern: impl Into<String>) {
        self.injection_patterns.push(pattern.into());
    }

    /// Sanitize user input.  Returns the input unchanged on success, or an
    /// error if the input fails validation.
    pub fn sanitize(&self, input: &str) -> Result<String, SecurityError> {
        // Length check
        if input.len() > self.max_length {
            return Err(SecurityError::InputRejected(format!(
                "input too long ({} chars, max {})",
                input.len(),
                self.max_length
            )));
        }

        // Injection detection
        let lower = input.to_lowercase();
        for pattern in &self.injection_patterns {
            if lower.contains(&pattern.to_lowercase()) {
                warn!(
                    pattern = pattern.as_str(),
                    "potential prompt injection detected"
                );
                return Err(SecurityError::InputRejected(format!(
                    "potential prompt injection detected: `{}`",
                    pattern
                )));
            }
        }

        Ok(input.to_string())
    }
}
