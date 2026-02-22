use crate::types::{LlmResponse, Message, Role, ToolCall};
use thiserror::Error;
use tracing::{debug, info};

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),
    #[error("Expected tool call '{expected}' but got '{actual}'")]
    ToolCallMismatch { expected: String, actual: String },
    #[error("Expected {expected} tool calls but got {actual}")]
    ToolCallCountMismatch { expected: usize, actual: usize },
    #[error("Response validation failed: {0}")]
    ResponseValidation(String),
    #[error("Behavior assertion failed: {0}")]
    BehaviorAssertion(String),
}

/// Constraint for validating agent behavior
#[derive(Debug, Clone)]
pub enum Constraint {
    /// Maximum number of turns allowed
    MaxTurns(usize),
    /// Required tool calls
    RequireToolCall(String),
    /// Forbidden tool calls
    ForbidToolCall(String),
    /// Response must contain text
    ResponseMustContain(String),
    /// Response must not contain text
    ResponseMustNotContain(String),
    /// Maximum response length
    MaxResponseLength(usize),
    /// Minimum response length
    MinResponseLength(usize),
    /// Custom validation function
    Custom {
        name: String,
        validator: fn(&ValidationContext) -> Result<(), String>,
    },
}

/// Context for validation
#[derive(Debug, Clone)]
pub struct ValidationContext {
    pub turns: Vec<Turn>,
    pub tool_calls: Vec<ToolCall>,
    pub responses: Vec<String>,
}

impl ValidationContext {
    pub fn new() -> Self {
        Self {
            turns: Vec::new(),
            tool_calls: Vec::new(),
            responses: Vec::new(),
        }
    }
    
    pub fn add_turn(&mut self, user_msg: Message, assistant_response: LlmResponse) {
        self.responses.push(assistant_response.content.clone());
        
        if !assistant_response.tool_calls.is_empty() {
            self.tool_calls.extend(assistant_response.tool_calls.clone());
        }
        
        self.turns.push(Turn {
            user_message: user_msg,
            assistant_response,
        });
    }
    
    pub fn turn_count(&self) -> usize {
        self.turns.len()
    }
    
    pub fn has_tool_call(&self, name: &str) -> bool {
        self.tool_calls.iter().any(|tc| tc.name == name)
    }
    
    pub fn tool_call_count(&self, name: &str) -> usize {
        self.tool_calls.iter().filter(|tc| tc.name == name).count()
    }
    
    pub fn last_response(&self) -> Option<&String> {
        self.responses.last()
    }
}

impl Default for ValidationContext {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct Turn {
    pub user_message: Message,
    pub assistant_response: LlmResponse,
}

/// Agent behavior validation harness
pub struct ValidationHarness {
    context: ValidationContext,
    constraints: Vec<Constraint>,
    strict_mode: bool,
}

impl ValidationHarness {
    pub fn new() -> Self {
        info!("ValidationHarness initialized");
        Self {
            context: ValidationContext::new(),
            constraints: Vec::new(),
            strict_mode: false,
        }
    }
    
    pub fn with_strict_mode(mut self, strict: bool) -> Self {
        self.strict_mode = strict;
        self
    }
    
    pub fn add_constraint(&mut self, constraint: Constraint) {
        debug!("Adding constraint: {:?}", constraint);
        self.constraints.push(constraint);
    }
    
    pub fn record_turn(&mut self, user_msg: Message, assistant_response: LlmResponse) {
        self.context.add_turn(user_msg, assistant_response);
    }
    
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        
        for constraint in &self.constraints {
            if let Err(e) = self.validate_constraint(constraint) {
                if self.strict_mode {
                    return Err(vec![e]);
                }
                errors.push(e);
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
    
    fn validate_constraint(&self, constraint: &Constraint) -> Result<(), ValidationError> {
        match constraint {
            Constraint::MaxTurns(max) => {
                if self.context.turn_count() > *max {
                    return Err(ValidationError::ConstraintViolation(
                        format!("Exceeded maximum turns: {} > {}", self.context.turn_count(), max)
                    ));
                }
            }
            Constraint::RequireToolCall(name) => {
                if !self.context.has_tool_call(name) {
                    return Err(ValidationError::ConstraintViolation(
                        format!("Required tool call '{}' was not made", name)
                    ));
                }
            }
            Constraint::ForbidToolCall(name) => {
                if self.context.has_tool_call(name) {
                    return Err(ValidationError::ConstraintViolation(
                        format!("Forbidden tool call '{}' was made", name)
                    ));
                }
            }
            Constraint::ResponseMustContain(text) => {
                let found = self.context.responses.iter().any(|r| r.contains(text));
                if !found {
                    return Err(ValidationError::ResponseValidation(
                        format!("No response contains '{}'", text)
                    ));
                }
            }
            Constraint::ResponseMustNotContain(text) => {
                let found = self.context.responses.iter().any(|r| r.contains(text));
                if found {
                    return Err(ValidationError::ResponseValidation(
                        format!("Response contains forbidden text '{}'", text)
                    ));
                }
            }
            Constraint::MaxResponseLength(max) => {
                for (i, response) in self.context.responses.iter().enumerate() {
                    if response.len() > *max {
                        return Err(ValidationError::ResponseValidation(
                            format!("Response {} exceeds max length: {} > {}", i, response.len(), max)
                        ));
                    }
                }
            }
            Constraint::MinResponseLength(min) => {
                for (i, response) in self.context.responses.iter().enumerate() {
                    if response.len() < *min {
                        return Err(ValidationError::ResponseValidation(
                            format!("Response {} below min length: {} < {}", i, response.len(), min)
                        ));
                    }
                }
            }
            Constraint::Custom { name, validator } => {
                if let Err(msg) = validator(&self.context) {
                    return Err(ValidationError::ConstraintViolation(
                        format!("Custom constraint '{}' failed: {}", name, msg)
                    ));
                }
            }
        }
        Ok(())
    }
    
    pub fn assert_tool_called(&self, name: &str) -> Result<(), ValidationError> {
        if !self.context.has_tool_call(name) {
            return Err(ValidationError::BehaviorAssertion(
                format!("Expected tool '{}' to be called", name)
            ));
        }
        Ok(())
    }
    
    pub fn assert_tool_call_count(&self, name: &str, expected: usize) -> Result<(), ValidationError> {
        let actual = self.context.tool_call_count(name);
        if actual != expected {
            return Err(ValidationError::ToolCallCountMismatch {
                expected,
                actual,
            });
        }
        Ok(())
    }
    
    pub fn assert_response_contains(&self, text: &str) -> Result<(), ValidationError> {
        let found = self.context.responses.iter().any(|r| r.contains(text));
        if !found {
            return Err(ValidationError::BehaviorAssertion(
                format!("Expected response to contain '{}'", text)
            ));
        }
        Ok(())
    }
    
    pub fn assert_no_tool_calls(&self) -> Result<(), ValidationError> {
        if !self.context.tool_calls.is_empty() {
            return Err(ValidationError::BehaviorAssertion(
                format!("Expected no tool calls, but {} were made", self.context.tool_calls.len())
            ));
        }
        Ok(())
    }
    
    pub fn get_context(&self) -> &ValidationContext {
        &self.context
    }
    
    pub fn reset(&mut self) {
        self.context = ValidationContext::new();
        debug!("ValidationHarness reset");
    }
}

impl Default for ValidationHarness {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating validation harnesses with constraints
pub struct ValidationHarnessBuilder {
    constraints: Vec<Constraint>,
    strict_mode: bool,
}

impl ValidationHarnessBuilder {
    pub fn new() -> Self {
        Self {
            constraints: Vec::new(),
            strict_mode: false,
        }
    }
    
    pub fn max_turns(mut self, max: usize) -> Self {
        self.constraints.push(Constraint::MaxTurns(max));
        self
    }
    
    pub fn require_tool(mut self, name: impl Into<String>) -> Self {
        self.constraints.push(Constraint::RequireToolCall(name.into()));
        self
    }
    
    pub fn forbid_tool(mut self, name: impl Into<String>) -> Self {
        self.constraints.push(Constraint::ForbidToolCall(name.into()));
        self
    }
    
    pub fn response_must_contain(mut self, text: impl Into<String>) -> Self {
        self.constraints.push(Constraint::ResponseMustContain(text.into()));
        self
    }
    
    pub fn response_must_not_contain(mut self, text: impl Into<String>) -> Self {
        self.constraints.push(Constraint::ResponseMustNotContain(text.into()));
        self
    }
    
    pub fn max_response_length(mut self, max: usize) -> Self {
        self.constraints.push(Constraint::MaxResponseLength(max));
        self
    }
    
    pub fn min_response_length(mut self, min: usize) -> Self {
        self.constraints.push(Constraint::MinResponseLength(min));
        self
    }
    
    pub fn strict(mut self) -> Self {
        self.strict_mode = true;
        self
    }
    
    pub fn build(self) -> ValidationHarness {
        let mut harness = ValidationHarness::new().with_strict_mode(self.strict_mode);
        for constraint in self.constraints {
            harness.add_constraint(constraint);
        }
        harness
    }
}

impl Default for ValidationHarnessBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FinishReason, Usage};

    fn create_test_response(content: &str, tool_calls: Option<Vec<ToolCall>>) -> LlmResponse {
        LlmResponse {
            content: content.to_string(),
            tool_calls: tool_calls.unwrap_or_default(),
            finish_reason: FinishReason::Stop,
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            }),
        }
    }

    #[test]
    fn test_validation_harness_basic() {
        let mut harness = ValidationHarness::new();
        
        let user_msg = Message::user("Hello");
        let response = create_test_response("Hi there!", None);
        
        harness.record_turn(user_msg, response);
        
        assert_eq!(harness.context.turn_count(), 1);
        assert!(harness.validate().is_ok());
    }

    #[test]
    fn test_max_turns_constraint() {
        let mut harness = ValidationHarnessBuilder::new()
            .max_turns(2)
            .build();
        
        for i in 0..3 {
            let user_msg = Message::user(&format!("Message {}", i));
            let response = create_test_response("Response", None);
            harness.record_turn(user_msg, response);
        }
        
        let result = harness.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_require_tool_call() {
        let mut harness = ValidationHarnessBuilder::new()
            .require_tool("calculator")
            .build();
        
        let user_msg = Message::user("What is 2+2?");
        let tool_call = ToolCall {
            id: "call_1".to_string(),
            name: "calculator".to_string(),
            arguments: serde_json::json!({}),
        };
        let response = create_test_response("Let me calculate that", Some(vec![tool_call]));
        
        harness.record_turn(user_msg, response);
        
        assert!(harness.validate().is_ok());
        assert!(harness.assert_tool_called("calculator").is_ok());
    }

    #[test]
    fn test_forbid_tool_call() {
        let mut harness = ValidationHarnessBuilder::new()
            .forbid_tool("dangerous_tool")
            .build();
        
        let user_msg = Message::user("Do something");
        let tool_call = ToolCall {
            id: "call_1".to_string(),
            name: "dangerous_tool".to_string(),
            arguments: serde_json::json!({}),
        };
        let response = create_test_response("Using dangerous tool", Some(vec![tool_call]));
        
        harness.record_turn(user_msg, response);
        
        let result = harness.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_response_must_contain() {
        let mut harness = ValidationHarnessBuilder::new()
            .response_must_contain("important keyword")
            .build();
        
        let user_msg = Message::user("Tell me something");
        let response = create_test_response("This contains the important keyword", None);
        
        harness.record_turn(user_msg, response);
        
        assert!(harness.validate().is_ok());
    }

    #[test]
    fn test_response_length_constraints() {
        let mut harness = ValidationHarnessBuilder::new()
            .min_response_length(10)
            .max_response_length(100)
            .build();
        
        let user_msg = Message::user("Question");
        let response = create_test_response("This is a good length response", None);
        
        harness.record_turn(user_msg, response);
        
        assert!(harness.validate().is_ok());
    }

    #[test]
    fn test_assert_tool_call_count() {
        let mut harness = ValidationHarness::new();
        
        let user_msg = Message::user("Calculate multiple things");
        let tool_calls = vec![
            ToolCall {
                id: "call_1".to_string(),
                name: "calculator".to_string(),
                arguments: serde_json::json!({}),
            },
            ToolCall {
                id: "call_2".to_string(),
                name: "calculator".to_string(),
                arguments: serde_json::json!({}),
            },
        ];
        let response = create_test_response("Calculating", Some(tool_calls));
        
        harness.record_turn(user_msg, response);
        
        assert!(harness.assert_tool_call_count("calculator", 2).is_ok());
        assert!(harness.assert_tool_call_count("calculator", 1).is_err());
    }

    #[test]
    fn test_strict_mode() {
        let mut harness = ValidationHarnessBuilder::new()
            .max_turns(1)
            .require_tool("tool1")
            .strict()
            .build();
        
        let user_msg = Message::user("Test");
        let response = create_test_response("Response", None);
        harness.record_turn(user_msg, response);
        
        // In strict mode, should fail on first constraint violation
        let result = harness.validate();
        assert!(result.is_err());
        if let Err(errors) = result {
            assert_eq!(errors.len(), 1); // Only first error in strict mode
        }
    }
}
