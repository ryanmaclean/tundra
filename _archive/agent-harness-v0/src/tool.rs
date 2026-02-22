use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tracing::{debug, error};

use crate::types::{ToolCall, ToolDefinition};

type AsyncToolFn = Box<
    dyn Fn(Value) -> Pin<Box<dyn std::future::Future<Output = Result<String, ToolError>> + Send>>
        + Send
        + Sync,
>;

/// Trait that any tool must implement.
#[async_trait]
pub trait Tool: Send + Sync {
    /// The tool's definition (name, description, JSON Schema parameters).
    fn definition(&self) -> ToolDefinition;

    /// Execute the tool with the given arguments and return a string result.
    async fn execute(&self, args: Value) -> Result<String, ToolError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    NotFound(String),
    #[error("Invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
}

/// Registry that holds all available tools.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool.
    pub fn register(&mut self, tool: impl Tool + 'static) {
        let name = tool.definition().name.clone();
        debug!("Registering tool: {}", name);
        self.tools.insert(name, Arc::new(tool));
    }

    /// Get all tool definitions for sending to the LLM.
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// Execute a tool call and return the result string.
    pub async fn execute(&self, call: &ToolCall) -> Result<String, ToolError> {
        let tool = self
            .tools
            .get(&call.name)
            .ok_or_else(|| ToolError::NotFound(call.name.clone()))?;

        debug!(
            "Executing tool '{}' with args: {}",
            call.name, call.arguments
        );
        let result = tool.execute(call.arguments.clone()).await;

        match &result {
            Ok(r) => debug!("Tool '{}' returned: {}", call.name, &r[..r.len().min(200)]),
            Err(e) => error!("Tool '{}' failed: {}", call.name, e),
        }

        result
    }

    /// Check if a tool is registered.
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Example: a simple closure-based tool for quick prototyping
// ---------------------------------------------------------------------------

/// A tool built from a closure, useful for quick prototyping.
pub struct FnTool<F>
where
    F: Fn(Value) -> Result<String, ToolError> + Send + Sync,
{
    def: ToolDefinition,
    func: F,
}

impl<F> FnTool<F>
where
    F: Fn(Value) -> Result<String, ToolError> + Send + Sync,
{
    pub fn new(def: ToolDefinition, func: F) -> Self {
        Self { def, func }
    }
}

#[async_trait]
impl<F> Tool for FnTool<F>
where
    F: Fn(Value) -> Result<String, ToolError> + Send + Sync,
{
    fn definition(&self) -> ToolDefinition {
        self.def.clone()
    }

    async fn execute(&self, args: Value) -> Result<String, ToolError> {
        (self.func)(args)
    }
}

/// A tool built from an async closure.
pub struct AsyncFnTool {
    def: ToolDefinition,
    func: AsyncToolFn,
}

impl AsyncFnTool {
    pub fn new<F, Fut>(def: ToolDefinition, func: F) -> Self
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<String, ToolError>> + Send + 'static,
    {
        Self {
            def,
            func: Box::new(move |args| Box::pin(func(args))),
        }
    }
}

#[async_trait]
impl Tool for AsyncFnTool {
    fn definition(&self) -> ToolDefinition {
        self.def.clone()
    }

    async fn execute(&self, args: Value) -> Result<String, ToolError> {
        (self.func)(args).await
    }
}
