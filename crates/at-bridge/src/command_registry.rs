use ahash::AHashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Command errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("command not found: {0}")]
    NotFound(String),

    #[error("invalid arguments: {0}")]
    InvalidArgs(String),

    #[error("execution failed: {0}")]
    ExecutionFailed(String),

    #[error("command disabled: {0}")]
    Disabled(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),
}

pub type Result<T> = std::result::Result<T, CommandError>;

// ---------------------------------------------------------------------------
// CommandContext — execution context passed to every command handler
// ---------------------------------------------------------------------------

/// Contextual information available to every command during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandContext {
    /// Which surface triggered this command (TUI, web, CLI, API, etc.)
    pub source: CommandSource,
    /// The raw arguments string passed to the command.
    pub args: String,
    /// Key-value parameters parsed from the args.
    pub params: AHashMap<String, serde_json::Value>,
}

impl CommandContext {
    pub fn new(source: CommandSource, args: impl Into<String>) -> Self {
        Self {
            source,
            args: args.into(),
            params: AHashMap::new(),
        }
    }

    pub fn with_param(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.params.insert(key.into(), value);
        self
    }

    /// Get a string parameter by key.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.params.get(key).and_then(|v| v.as_str())
    }

    /// Get a u64 parameter by key.
    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.params.get(key).and_then(|v| v.as_u64())
    }

    /// Get a bool parameter by key.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.params.get(key).and_then(|v| v.as_bool())
    }
}

// ---------------------------------------------------------------------------
// CommandSource — where the command was triggered from
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandSource {
    Tui,
    Web,
    Cli,
    Api,
    Keybinding,
    Plugin,
    Internal,
}

impl std::fmt::Display for CommandSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandSource::Tui => write!(f, "tui"),
            CommandSource::Web => write!(f, "web"),
            CommandSource::Cli => write!(f, "cli"),
            CommandSource::Api => write!(f, "api"),
            CommandSource::Keybinding => write!(f, "keybinding"),
            CommandSource::Plugin => write!(f, "plugin"),
            CommandSource::Internal => write!(f, "internal"),
        }
    }
}

// ---------------------------------------------------------------------------
// CommandResult — what a command returns
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandOutput {
    pub success: bool,
    pub message: Option<String>,
    pub data: Option<serde_json::Value>,
}

impl CommandOutput {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: Some(message.into()),
            data: None,
        }
    }

    pub fn ok_data(data: serde_json::Value) -> Self {
        Self {
            success: true,
            message: None,
            data: Some(data),
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: Some(message.into()),
            data: None,
        }
    }
}

// ---------------------------------------------------------------------------
// CommandHandler trait — what command implementations provide
// ---------------------------------------------------------------------------

#[async_trait]
pub trait CommandHandler: Send + Sync + 'static {
    /// Execute the command with the given context.
    async fn execute(&self, ctx: CommandContext) -> Result<CommandOutput>;
}

// ---------------------------------------------------------------------------
// CommandDescriptor — metadata about a registered command
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDescriptor {
    /// Unique command name (e.g., "bead.create", "agent.stop").
    pub name: String,
    /// Human-readable title for display in palettes.
    pub title: String,
    /// Short description for help text.
    pub description: String,
    /// Category for grouping in the command palette.
    pub category: CommandCategory,
    /// Keyboard shortcut (if any).
    pub keybinding: Option<String>,
    /// Which surfaces this command is available from.
    pub available_from: Vec<CommandSource>,
    /// Whether the command is currently enabled.
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandCategory {
    Bead,
    Agent,
    Session,
    Git,
    Navigation,
    View,
    System,
    Plugin,
}

impl std::fmt::Display for CommandCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandCategory::Bead => write!(f, "Bead"),
            CommandCategory::Agent => write!(f, "Agent"),
            CommandCategory::Session => write!(f, "Session"),
            CommandCategory::Git => write!(f, "Git"),
            CommandCategory::Navigation => write!(f, "Navigation"),
            CommandCategory::View => write!(f, "View"),
            CommandCategory::System => write!(f, "System"),
            CommandCategory::Plugin => write!(f, "Plugin"),
        }
    }
}

// ---------------------------------------------------------------------------
// CommandEntry — descriptor + handler stored together
// ---------------------------------------------------------------------------

struct CommandEntry {
    descriptor: CommandDescriptor,
    handler: Arc<dyn CommandHandler>,
}

// ---------------------------------------------------------------------------
// CommandRegistry — the central dispatch table
// ---------------------------------------------------------------------------

/// A unified command registry inspired by Lapce's command system.
///
/// All actions in auto-tundra — whether triggered from TUI keybindings,
/// the web dashboard, CLI, or API — are registered as named commands.
/// This gives us:
/// - A command palette (fuzzy search over all commands)
/// - Keybinding support (map keys to command names)
/// - Surface-agnostic execution (same command works from TUI, web, CLI)
/// - Discoverability (list all available commands with descriptions)
pub struct CommandRegistry {
    commands: AHashMap<String, CommandEntry>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: AHashMap::new(),
        }
    }

    /// Register a command with its descriptor and handler.
    pub fn register(&mut self, descriptor: CommandDescriptor, handler: Arc<dyn CommandHandler>) {
        let name = descriptor.name.clone();
        self.commands.insert(
            name,
            CommandEntry {
                descriptor,
                handler,
            },
        );
    }

    /// Execute a command by name.
    pub async fn execute(&self, name: &str, ctx: CommandContext) -> Result<CommandOutput> {
        let entry = self
            .commands
            .get(name)
            .ok_or_else(|| CommandError::NotFound(name.to_string()))?;

        if !entry.descriptor.enabled {
            return Err(CommandError::Disabled(name.to_string()));
        }

        if !entry.descriptor.available_from.is_empty()
            && !entry.descriptor.available_from.contains(&ctx.source)
        {
            return Err(CommandError::PermissionDenied(format!(
                "'{}' not available from {}",
                name, ctx.source,
            )));
        }

        entry.handler.execute(ctx).await
    }

    /// Get descriptor for a command by name.
    pub fn get_descriptor(&self, name: &str) -> Option<&CommandDescriptor> {
        self.commands.get(name).map(|e| &e.descriptor)
    }

    /// List all registered command descriptors.
    pub fn list(&self) -> Vec<&CommandDescriptor> {
        self.commands.values().map(|e| &e.descriptor).collect()
    }

    /// List commands in a specific category.
    pub fn by_category(&self, category: CommandCategory) -> Vec<&CommandDescriptor> {
        self.commands
            .values()
            .filter(|e| e.descriptor.category == category)
            .map(|e| &e.descriptor)
            .collect()
    }

    /// Fuzzy search commands by name or title for command palette.
    pub fn search(&self, query: &str) -> Vec<&CommandDescriptor> {
        let q = query.to_lowercase();
        let mut matches: Vec<(&CommandDescriptor, usize)> = self
            .commands
            .values()
            .filter_map(|e| {
                let name_match = e.descriptor.name.to_lowercase().contains(&q);
                let title_match = e.descriptor.title.to_lowercase().contains(&q);
                let desc_match = e.descriptor.description.to_lowercase().contains(&q);

                if name_match {
                    Some((&e.descriptor, 0)) // best match
                } else if title_match {
                    Some((&e.descriptor, 1))
                } else if desc_match {
                    Some((&e.descriptor, 2))
                } else {
                    None
                }
            })
            .collect();

        matches.sort_by_key(|(_, score)| *score);
        matches.into_iter().map(|(d, _)| d).collect()
    }

    /// Find the command bound to a keybinding.
    pub fn by_keybinding(&self, keybinding: &str) -> Option<&CommandDescriptor> {
        self.commands
            .values()
            .find(|e| e.descriptor.keybinding.as_deref() == Some(keybinding))
            .map(|e| &e.descriptor)
    }

    /// Filter commands available from a specific source.
    pub fn available_from(&self, source: CommandSource) -> Vec<&CommandDescriptor> {
        self.commands
            .values()
            .filter(|e| {
                e.descriptor.available_from.is_empty()
                    || e.descriptor.available_from.contains(&source)
            })
            .map(|e| &e.descriptor)
            .collect()
    }

    /// Enable or disable a command by name.
    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> bool {
        if let Some(entry) = self.commands.get_mut(name) {
            entry.descriptor.enabled = enabled;
            true
        } else {
            false
        }
    }

    /// Total number of registered commands.
    pub fn count(&self) -> usize {
        self.commands.len()
    }

    /// Check if a command is registered.
    pub fn has(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// FnCommandHandler — wrap a closure as a command handler
// ---------------------------------------------------------------------------

/// Wraps an async function as a CommandHandler for simple commands.
pub struct FnCommandHandler<F>
where
    F: Fn(
            CommandContext,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<CommandOutput>> + Send>>
        + Send
        + Sync
        + 'static,
{
    f: F,
}

impl<F> FnCommandHandler<F>
where
    F: Fn(
            CommandContext,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<CommandOutput>> + Send>>
        + Send
        + Sync
        + 'static,
{
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

#[async_trait]
impl<F> CommandHandler for FnCommandHandler<F>
where
    F: Fn(
            CommandContext,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<CommandOutput>> + Send>>
        + Send
        + Sync
        + 'static,
{
    async fn execute(&self, ctx: CommandContext) -> Result<CommandOutput> {
        (self.f)(ctx).await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct EchoHandler;

    #[async_trait]
    impl CommandHandler for EchoHandler {
        async fn execute(&self, ctx: CommandContext) -> Result<CommandOutput> {
            Ok(CommandOutput::ok(format!("echo: {}", ctx.args)))
        }
    }

    fn test_descriptor(name: &str, category: CommandCategory) -> CommandDescriptor {
        CommandDescriptor {
            name: name.to_string(),
            title: format!("Test {}", name),
            description: format!("Test command {}", name),
            category,
            keybinding: None,
            available_from: vec![],
            enabled: true,
        }
    }

    #[tokio::test]
    async fn register_and_execute() {
        let mut reg = CommandRegistry::new();
        reg.register(
            test_descriptor("test.echo", CommandCategory::System),
            Arc::new(EchoHandler),
        );

        let ctx = CommandContext::new(CommandSource::Tui, "hello");
        let output = reg.execute("test.echo", ctx).await.unwrap();
        assert!(output.success);
        assert_eq!(output.message.as_deref(), Some("echo: hello"));
    }

    #[tokio::test]
    async fn command_not_found() {
        let reg = CommandRegistry::new();
        let ctx = CommandContext::new(CommandSource::Tui, "");
        let result = reg.execute("nonexistent", ctx).await;
        assert!(matches!(result, Err(CommandError::NotFound(_))));
    }

    #[tokio::test]
    async fn disabled_command_rejected() {
        let mut reg = CommandRegistry::new();
        reg.register(
            test_descriptor("test.disabled", CommandCategory::System),
            Arc::new(EchoHandler),
        );
        reg.set_enabled("test.disabled", false);

        let ctx = CommandContext::new(CommandSource::Tui, "");
        let result = reg.execute("test.disabled", ctx).await;
        assert!(matches!(result, Err(CommandError::Disabled(_))));
    }

    #[tokio::test]
    async fn source_restriction() {
        let mut reg = CommandRegistry::new();
        let mut desc = test_descriptor("test.tui_only", CommandCategory::View);
        desc.available_from = vec![CommandSource::Tui];
        reg.register(desc, Arc::new(EchoHandler));

        // TUI works
        let ctx = CommandContext::new(CommandSource::Tui, "");
        assert!(reg.execute("test.tui_only", ctx).await.is_ok());

        // Web denied
        let ctx = CommandContext::new(CommandSource::Web, "");
        let result = reg.execute("test.tui_only", ctx).await;
        assert!(matches!(result, Err(CommandError::PermissionDenied(_))));
    }

    #[test]
    fn list_commands() {
        let mut reg = CommandRegistry::new();
        reg.register(
            test_descriptor("a.one", CommandCategory::Bead),
            Arc::new(EchoHandler),
        );
        reg.register(
            test_descriptor("b.two", CommandCategory::Agent),
            Arc::new(EchoHandler),
        );
        assert_eq!(reg.count(), 2);
        assert_eq!(reg.list().len(), 2);
    }

    #[test]
    fn by_category() {
        let mut reg = CommandRegistry::new();
        reg.register(
            test_descriptor("bead.create", CommandCategory::Bead),
            Arc::new(EchoHandler),
        );
        reg.register(
            test_descriptor("agent.stop", CommandCategory::Agent),
            Arc::new(EchoHandler),
        );
        reg.register(
            test_descriptor("bead.delete", CommandCategory::Bead),
            Arc::new(EchoHandler),
        );
        assert_eq!(reg.by_category(CommandCategory::Bead).len(), 2);
        assert_eq!(reg.by_category(CommandCategory::Agent).len(), 1);
        assert_eq!(reg.by_category(CommandCategory::Git).len(), 0);
    }

    #[test]
    fn search_commands() {
        let mut reg = CommandRegistry::new();
        reg.register(
            test_descriptor("bead.create", CommandCategory::Bead),
            Arc::new(EchoHandler),
        );
        reg.register(
            test_descriptor("agent.stop", CommandCategory::Agent),
            Arc::new(EchoHandler),
        );

        let results = reg.search("bead");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "bead.create");

        let results = reg.search("test");
        assert_eq!(results.len(), 2); // both titles contain "Test"
    }

    #[test]
    fn keybinding_lookup() {
        let mut reg = CommandRegistry::new();
        let mut desc = test_descriptor("view.toggle", CommandCategory::View);
        desc.keybinding = Some("ctrl+t".to_string());
        reg.register(desc, Arc::new(EchoHandler));

        assert_eq!(
            reg.by_keybinding("ctrl+t").map(|d| d.name.as_str()),
            Some("view.toggle"),
        );
        assert!(reg.by_keybinding("ctrl+x").is_none());
    }

    #[test]
    fn available_from_filter() {
        let mut reg = CommandRegistry::new();

        let mut desc = test_descriptor("tui.only", CommandCategory::View);
        desc.available_from = vec![CommandSource::Tui];
        reg.register(desc, Arc::new(EchoHandler));

        reg.register(
            test_descriptor("all.access", CommandCategory::System),
            Arc::new(EchoHandler),
        );

        // TUI sees both (empty available_from = all)
        assert_eq!(reg.available_from(CommandSource::Tui).len(), 2);
        // Web only sees the unrestricted one
        assert_eq!(reg.available_from(CommandSource::Web).len(), 1);
    }

    #[test]
    fn command_context_params() {
        let ctx = CommandContext::new(CommandSource::Cli, "test")
            .with_param("name", serde_json::json!("hello"))
            .with_param("count", serde_json::json!(42))
            .with_param("verbose", serde_json::json!(true));

        assert_eq!(ctx.get_str("name"), Some("hello"));
        assert_eq!(ctx.get_u64("count"), Some(42));
        assert_eq!(ctx.get_bool("verbose"), Some(true));
        assert!(ctx.get_str("missing").is_none());
    }

    #[test]
    fn command_output_constructors() {
        let ok = CommandOutput::ok("done");
        assert!(ok.success);
        assert_eq!(ok.message.as_deref(), Some("done"));

        let data = CommandOutput::ok_data(serde_json::json!({"key": "val"}));
        assert!(data.success);
        assert!(data.data.is_some());

        let err = CommandOutput::err("bad");
        assert!(!err.success);
    }

    #[test]
    fn has_and_get_descriptor() {
        let mut reg = CommandRegistry::new();
        reg.register(
            test_descriptor("sys.info", CommandCategory::System),
            Arc::new(EchoHandler),
        );
        assert!(reg.has("sys.info"));
        assert!(!reg.has("nonexistent"));
        assert!(reg.get_descriptor("sys.info").is_some());
    }

    #[test]
    fn command_source_display() {
        assert_eq!(CommandSource::Tui.to_string(), "tui");
        assert_eq!(CommandSource::Web.to_string(), "web");
        assert_eq!(CommandSource::Keybinding.to_string(), "keybinding");
    }

    #[test]
    fn command_category_display() {
        assert_eq!(CommandCategory::Bead.to_string(), "Bead");
        assert_eq!(CommandCategory::Git.to_string(), "Git");
        assert_eq!(CommandCategory::Plugin.to_string(), "Plugin");
    }
}
