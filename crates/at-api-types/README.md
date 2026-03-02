# at-api-types

Shared API type definitions for auto-tundra services.

## Overview

This crate provides common type definitions used across multiple auto-tundra services to ensure consistency in API requests and responses. By centralizing these types, we:

- **Reduce code duplication** across services
- **Ensure consistency** in API contracts
- **Simplify maintenance** of shared data structures
- **Enable type-safe** communication between services

## Usage

Add this crate as a dependency:

```toml
[dependencies]
at-api-types = { path = "../../crates/at-api-types" }
```

Then import the types you need:

```rust
use at_api_types::{ApiBead, ApiAgent, CreateBeadRequest};
```

## Type Categories

### Response Types

Common API response structures:
- `ApiBead` - Task/bead information
- `ApiAgent` - Agent status and configuration
- `ApiKpi` - Key performance indicators
- `ApiSession` - Session information
- `ApiConvoy` - Convoy (task group) details
- `ApiWorktree` - Git worktree information
- `ApiCosts` - Token usage and cost tracking
- `ApiMcpServer` - MCP server configuration
- `ApiMemoryEntry` - Memory/knowledge base entries
- `ApiRoadmap*` - Roadmap and feature planning
- `ApiIdea` - Feature ideas and suggestions
- `ApiStack*` - Stacked diff information
- `ApiGithub*` - GitHub integration types
- And more...

### Request Types

Common API request structures:
- `CreateBeadRequest` - Create new tasks
- `AddMemoryRequest` - Add memory entries
- `CreateTaskRequest` - Create tasks with full metadata
- `AddMcpServerRequest` - Configure MCP servers
- `PublishGithubReleaseRequest` - GitHub release publishing
- `CreateProjectRequest` / `UpdateProjectRequest` - Project management
- And more...

## Design Principles

All types in this crate:
- Use `#[derive(Debug, Clone, Serialize, Deserialize)]` for consistency
- Apply `#[serde(default)]` to optional fields for robustness
- Use `#[serde(skip_serializing_if = "Option::is_none")]` for optional request fields
- Follow naming convention: `Api*` for response types, `*Request` for request types

## Services Using This Crate

- `leptos-ui` - Web frontend
- `daemon` - Backend API server
- Other auto-tundra services (as needed)

## Contributing

When adding new types:
1. Follow the existing naming conventions
2. Add appropriate serde attributes
3. Document the purpose of complex types
4. Run `cargo build -p at-api-types` to verify
5. Update this README if adding new type categories
