use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// MCP Protocol Types (Model Context Protocol)
// Implements the core JSON-RPC based protocol for tool/resource/prompt exchange
// between MCP servers and clients.
// ---------------------------------------------------------------------------

/// MCP protocol version.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

// ---------------------------------------------------------------------------
// JSON-RPC Transport
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(serde_json::Value::Number(1.into())),
            method: method.into(),
            params,
        }
    }

    pub fn notification(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn success(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<serde_json::Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }

    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Standard JSON-RPC error codes.
pub mod error_codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
}

// ---------------------------------------------------------------------------
// MCP Tool Definition
// ---------------------------------------------------------------------------

/// An MCP tool that can be called by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// Unique tool name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema for input parameters.
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
    /// MCP tool annotations (hints for the LLM).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<ToolAnnotations>,
}

/// MCP tool annotations — hints about tool behavior.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolAnnotations {
    /// Tool only reads data, doesn't modify state.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "readOnlyHint"
    )]
    pub read_only_hint: Option<bool>,
    /// Tool may perform destructive actions.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "destructiveHint"
    )]
    pub destructive_hint: Option<bool>,
    /// Calling tool multiple times with same args has same effect.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "idempotentHint"
    )]
    pub idempotent_hint: Option<bool>,
    /// Tool interacts with external world (network, etc).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "openWorldHint"
    )]
    pub open_world_hint: Option<bool>,
}

// ---------------------------------------------------------------------------
// MCP Resource
// ---------------------------------------------------------------------------

/// An MCP resource — a read-only data source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    /// URI identifying this resource.
    pub uri: String,
    /// Human-readable name.
    pub name: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MIME type of the resource content.
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "mimeType")]
    pub mime_type: Option<String>,
}

/// Content of a resource.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "mimeType")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

// ---------------------------------------------------------------------------
// MCP Prompt
// ---------------------------------------------------------------------------

/// An MCP prompt template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPrompt {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub arguments: Vec<PromptArgument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptArgument {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
}

// ---------------------------------------------------------------------------
// MCP Server Capabilities
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolsCapability {
    #[serde(default, rename = "listChanged")]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourcesCapability {
    #[serde(default)]
    pub subscribe: bool,
    #[serde(default, rename = "listChanged")]
    pub list_changed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptsCapability {
    #[serde(default, rename = "listChanged")]
    pub list_changed: bool,
}

// ---------------------------------------------------------------------------
// MCP Server Info
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

// ---------------------------------------------------------------------------
// MCP Server Config — for connecting to external MCP servers
// ---------------------------------------------------------------------------

/// Configuration for connecting to an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Human-readable name.
    pub name: String,
    /// Transport type.
    pub transport: McpTransport,
    /// Whether this server is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Environment variables to pass to stdio transport.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_true() -> bool {
    true
}

/// MCP transport configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransport {
    /// Stdio transport — spawn a child process.
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    /// SSE transport — connect to HTTP endpoint.
    Sse { url: String },
    /// Streamable HTTP transport (MCP 2025+).
    StreamableHttp { url: String },
}

// ---------------------------------------------------------------------------
// MCP Tool Registry — manages available tools from multiple servers
// ---------------------------------------------------------------------------

/// Tracks tools, resources, and prompts from multiple MCP servers.
#[derive(Debug)]
pub struct McpToolRegistry {
    /// Tools keyed by "server_name/tool_name".
    tools: HashMap<String, RegisteredTool>,
    /// Resources keyed by URI.
    resources: HashMap<String, RegisteredResource>,
    /// Prompts keyed by "server_name/prompt_name".
    prompts: HashMap<String, RegisteredPrompt>,
}

#[derive(Debug, Clone)]
pub struct RegisteredTool {
    pub server: String,
    pub tool: McpTool,
}

#[derive(Debug, Clone)]
pub struct RegisteredResource {
    pub server: String,
    pub resource: McpResource,
}

#[derive(Debug, Clone)]
pub struct RegisteredPrompt {
    pub server: String,
    pub prompt: McpPrompt,
}

impl McpToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            resources: HashMap::new(),
            prompts: HashMap::new(),
        }
    }

    /// Register tools from an MCP server.
    pub fn register_tools(&mut self, server_name: &str, tools: Vec<McpTool>) {
        for tool in tools {
            let key = format!("{}/{}", server_name, tool.name);
            debug!(key = %key, "registered MCP tool");
            self.tools.insert(
                key,
                RegisteredTool {
                    server: server_name.to_string(),
                    tool,
                },
            );
        }
    }

    /// Register resources from an MCP server.
    pub fn register_resources(&mut self, server_name: &str, resources: Vec<McpResource>) {
        for resource in resources {
            let key = resource.uri.clone();
            self.resources.insert(
                key,
                RegisteredResource {
                    server: server_name.to_string(),
                    resource,
                },
            );
        }
    }

    /// Register prompts from an MCP server.
    pub fn register_prompts(&mut self, server_name: &str, prompts: Vec<McpPrompt>) {
        for prompt in prompts {
            let key = format!("{}/{}", server_name, prompt.name);
            self.prompts.insert(
                key,
                RegisteredPrompt {
                    server: server_name.to_string(),
                    prompt,
                },
            );
        }
    }

    /// Remove all tools/resources/prompts from a server.
    pub fn unregister_server(&mut self, server_name: &str) {
        self.tools.retain(|_, v| v.server != server_name);
        self.resources.retain(|_, v| v.server != server_name);
        self.prompts.retain(|_, v| v.server != server_name);
        info!(server = server_name, "unregistered MCP server");
    }

    /// Get a tool by its qualified name ("server/tool").
    pub fn get_tool(&self, key: &str) -> Option<&RegisteredTool> {
        self.tools.get(key)
    }

    /// Find a tool by just the tool name (returns first match).
    pub fn find_tool_by_name(&self, tool_name: &str) -> Option<&RegisteredTool> {
        self.tools.values().find(|rt| rt.tool.name == tool_name)
    }

    /// List all registered tools.
    pub fn list_tools(&self) -> Vec<&RegisteredTool> {
        self.tools.values().collect()
    }

    /// List tools from a specific server.
    pub fn list_tools_for_server(&self, server_name: &str) -> Vec<&RegisteredTool> {
        self.tools
            .values()
            .filter(|rt| rt.server == server_name)
            .collect()
    }

    /// Get a resource by URI.
    pub fn get_resource(&self, uri: &str) -> Option<&RegisteredResource> {
        self.resources.get(uri)
    }

    /// List all resources.
    pub fn list_resources(&self) -> Vec<&RegisteredResource> {
        self.resources.values().collect()
    }

    /// List all prompts.
    pub fn list_prompts(&self) -> Vec<&RegisteredPrompt> {
        self.prompts.values().collect()
    }

    /// Total number of registered tools.
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    /// Total number of registered resources.
    pub fn resource_count(&self) -> usize {
        self.resources.len()
    }

    /// Total number of registered prompts.
    pub fn prompt_count(&self) -> usize {
        self.prompts.len()
    }

    /// Convert all tools to the format expected by LLM providers.
    pub fn tools_for_llm(&self) -> Vec<crate::provider::Tool> {
        self.tools
            .values()
            .map(|rt| crate::provider::Tool {
                name: rt.tool.name.clone(),
                description: rt.tool.description.clone(),
                parameters: rt.tool.input_schema.clone(),
            })
            .collect()
    }

    /// List server names that have registered tools.
    pub fn server_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.values().map(|rt| rt.server.clone()).collect();
        names.sort();
        names.dedup();
        names
    }
}

impl Default for McpToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl McpToolRegistry {
    /// Create a new registry with built-in Tundra Tools pre-registered.
    pub fn with_builtins() -> Self {
        let mut reg = Self::new();
        reg.register_builtin_tools();
        reg
    }

    /// Register the always-enabled built-in Tundra Tools.
    pub fn register_builtin_tools(&mut self) {
        let tools = crate::builtin_tools::builtin_tool_definitions();
        let server_name = crate::builtin_tools::BUILTIN_SERVER_NAME;
        info!(
            server = server_name,
            count = tools.len(),
            "registering built-in tools"
        );
        self.register_tools(server_name, tools);
    }
}

// ---------------------------------------------------------------------------
// MCP Call/Result types
// ---------------------------------------------------------------------------

/// Request to call an MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

/// Result of an MCP tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    #[serde(default)]
    pub content: Vec<ToolResultContent>,
    #[serde(default, rename = "isError")]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolResultContent {
    Text { text: String },
    Image { data: String, mime_type: String },
    Resource { resource: ResourceContent },
}

impl ToolCallResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolResultContent::Text { text: text.into() }],
            is_error: false,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ToolResultContent::Text {
                text: message.into(),
            }],
            is_error: true,
        }
    }

    /// Extract the first text content.
    pub fn text_content(&self) -> Option<&str> {
        self.content.iter().find_map(|c| match c {
            ToolResultContent::Text { text } => Some(text.as_str()),
            _ => None,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tool(name: &str) -> McpTool {
        McpTool {
            name: name.to_string(),
            description: format!("Tool {}", name),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
            annotations: None,
        }
    }

    fn sample_resource(uri: &str) -> McpResource {
        McpResource {
            uri: uri.to_string(),
            name: uri.to_string(),
            description: None,
            mime_type: Some("text/plain".to_string()),
        }
    }

    fn sample_prompt(name: &str) -> McpPrompt {
        McpPrompt {
            name: name.to_string(),
            description: Some(format!("Prompt {}", name)),
            arguments: vec![],
        }
    }

    // -- JSON-RPC --

    #[test]
    fn jsonrpc_request_serialization() {
        let req = JsonRpcRequest::new("tools/list", None);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"tools/list\""));
    }

    #[test]
    fn jsonrpc_notification_has_no_id() {
        let notif = JsonRpcRequest::notification("notifications/initialized", None);
        assert!(notif.id.is_none());
    }

    #[test]
    fn jsonrpc_response_success() {
        let resp = JsonRpcResponse::success(
            Some(serde_json::Value::Number(1.into())),
            serde_json::json!({"tools": []}),
        );
        assert!(!resp.is_error());
        assert!(resp.result.is_some());
    }

    #[test]
    fn jsonrpc_response_error() {
        let resp = JsonRpcResponse::error(
            Some(serde_json::Value::Number(1.into())),
            error_codes::METHOD_NOT_FOUND,
            "Method not found",
        );
        assert!(resp.is_error());
        assert_eq!(resp.error.as_ref().unwrap().code, -32601);
    }

    // -- MCP Tool --

    #[test]
    fn mcp_tool_serialization() {
        let tool = McpTool {
            name: "file_read".to_string(),
            description: "Read a file".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
            annotations: Some(ToolAnnotations {
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
            }),
        };
        let json = serde_json::to_string(&tool).unwrap();
        let parsed: McpTool = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "file_read");
        assert_eq!(parsed.annotations.unwrap().read_only_hint, Some(true));
    }

    #[test]
    fn tool_annotations_default() {
        let ann = ToolAnnotations::default();
        assert!(ann.read_only_hint.is_none());
        assert!(ann.destructive_hint.is_none());
    }

    // -- Tool Registry --

    #[test]
    fn registry_register_and_list_tools() {
        let mut reg = McpToolRegistry::new();
        reg.register_tools("ctx7", vec![sample_tool("search"), sample_tool("read")]);
        assert_eq!(reg.tool_count(), 2);
        assert!(reg.get_tool("ctx7/search").is_some());
        assert!(reg.get_tool("ctx7/read").is_some());
    }

    #[test]
    fn registry_find_tool_by_name() {
        let mut reg = McpToolRegistry::new();
        reg.register_tools("github", vec![sample_tool("create_issue")]);
        let found = reg.find_tool_by_name("create_issue").unwrap();
        assert_eq!(found.server, "github");
    }

    #[test]
    fn registry_find_missing_tool() {
        let reg = McpToolRegistry::new();
        assert!(reg.find_tool_by_name("nonexistent").is_none());
    }

    #[test]
    fn registry_list_tools_for_server() {
        let mut reg = McpToolRegistry::new();
        reg.register_tools("a", vec![sample_tool("t1"), sample_tool("t2")]);
        reg.register_tools("b", vec![sample_tool("t3")]);
        assert_eq!(reg.list_tools_for_server("a").len(), 2);
        assert_eq!(reg.list_tools_for_server("b").len(), 1);
        assert_eq!(reg.list_tools_for_server("c").len(), 0);
    }

    #[test]
    fn registry_unregister_server() {
        let mut reg = McpToolRegistry::new();
        reg.register_tools("s1", vec![sample_tool("t1")]);
        reg.register_resources("s1", vec![sample_resource("file:///a")]);
        reg.register_prompts("s1", vec![sample_prompt("p1")]);
        assert_eq!(reg.tool_count(), 1);

        reg.unregister_server("s1");
        assert_eq!(reg.tool_count(), 0);
        assert_eq!(reg.resource_count(), 0);
        assert_eq!(reg.prompt_count(), 0);
    }

    #[test]
    fn registry_resources() {
        let mut reg = McpToolRegistry::new();
        reg.register_resources("fs", vec![sample_resource("file:///tmp/a.txt")]);
        assert_eq!(reg.resource_count(), 1);
        assert!(reg.get_resource("file:///tmp/a.txt").is_some());
    }

    #[test]
    fn registry_prompts() {
        let mut reg = McpToolRegistry::new();
        reg.register_prompts("sys", vec![sample_prompt("summarize")]);
        assert_eq!(reg.prompt_count(), 1);
        assert_eq!(reg.list_prompts()[0].prompt.name, "summarize");
    }

    #[test]
    fn registry_server_names() {
        let mut reg = McpToolRegistry::new();
        reg.register_tools("github", vec![sample_tool("t1")]);
        reg.register_tools("ctx7", vec![sample_tool("t2")]);
        reg.register_tools("github", vec![sample_tool("t3")]);
        let names = reg.server_names();
        assert_eq!(names, vec!["ctx7", "github"]);
    }

    #[test]
    fn registry_tools_for_llm() {
        let mut reg = McpToolRegistry::new();
        reg.register_tools("s", vec![sample_tool("read_file")]);
        let llm_tools = reg.tools_for_llm();
        assert_eq!(llm_tools.len(), 1);
        assert_eq!(llm_tools[0].name, "read_file");
    }

    #[test]
    fn registry_default_is_empty() {
        let reg = McpToolRegistry::default();
        assert_eq!(reg.tool_count(), 0);
    }

    // -- MCP Server Config --

    #[test]
    fn mcp_server_config_stdio_serialization() {
        let config = McpServerConfig {
            name: "context7".to_string(),
            transport: McpTransport::Stdio {
                command: "npx".to_string(),
                args: vec!["-y".to_string(), "@context7/mcp".to_string()],
            },
            enabled: true,
            env: HashMap::new(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: McpServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "context7");
        assert!(parsed.enabled);
    }

    #[test]
    fn mcp_server_config_sse_serialization() {
        let config = McpServerConfig {
            name: "remote".to_string(),
            transport: McpTransport::Sse {
                url: "http://localhost:8080/sse".to_string(),
            },
            enabled: false,
            env: HashMap::new(),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"sse\""));
        let parsed: McpServerConfig = serde_json::from_str(&json).unwrap();
        assert!(!parsed.enabled);
    }

    // -- Tool Call --

    #[test]
    fn tool_call_result_text() {
        let result = ToolCallResult::text("file contents here");
        assert!(!result.is_error);
        assert_eq!(result.text_content(), Some("file contents here"));
    }

    #[test]
    fn tool_call_result_error() {
        let result = ToolCallResult::error("file not found");
        assert!(result.is_error);
        assert_eq!(result.text_content(), Some("file not found"));
    }

    #[test]
    fn tool_call_result_serialization() {
        let result = ToolCallResult::text("hello");
        let json = serde_json::to_string(&result).unwrap();
        let parsed: ToolCallResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.text_content(), Some("hello"));
        assert!(!parsed.is_error);
    }

    #[test]
    fn initialize_result_serialization() {
        let result = InitializeResult {
            protocol_version: MCP_PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability { list_changed: true }),
                resources: None,
                prompts: None,
            },
            server_info: ServerInfo {
                name: "auto-tundra".to_string(),
                version: "0.1.0".to_string(),
            },
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: InitializeResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.protocol_version, MCP_PROTOCOL_VERSION);
        assert_eq!(parsed.server_info.name, "auto-tundra");
        assert!(parsed.capabilities.tools.unwrap().list_changed);
    }
}
