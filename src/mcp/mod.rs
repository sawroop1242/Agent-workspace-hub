//! MCP (Model Context Protocol) integration and enforcement.
//!
//! This module implements the security boundary around MCP tool servers: trust
//! and approval policy, permission validation, sandboxed process execution,
//! argument/schema validation, and bounded transport. Every boundary fails
//! closed — if a protection cannot be applied, the operation is rejected.

/// Structured security audit logging.
pub mod audit;
/// Bearer-token authentication for the remote transport.
pub mod auth;
/// Circuit breaker for repeated MCP provider failures.
pub mod circuit_breaker;
/// CLI trust commands.
pub mod cli_trust;
/// Community MCP registry client.
pub mod community_registry;
/// Composio provider integration.
pub mod composio;
/// Composio authentication.
pub mod composio_auth;
/// Global registry of Composio connected accounts.
pub mod composio_registry;
/// Runtime configuration and resource limits with precedence rules.
pub mod config;
/// Connector metadata store.
pub mod connectors;
/// Workspace context assembly.
pub mod context;
/// Custom (per-project) MCP servers.
pub mod custom_mcp;
/// Transport-agnostic MCP JSON-RPC dispatcher (shared by all transports).
pub mod dispatcher;
/// Structured authorization/security errors.
pub mod error;
/// MCP execution authorization gate.
pub mod execution_gate;
/// First-class GitHub provider (`github.*` tools).
pub mod github;
/// Globally installed MCP servers.
pub mod global_mcp;
/// HTTP/SSE remote transport server.
pub mod http;
/// MCP memory store.
pub mod memory;
/// Lifecycle events, bounded observer hooks, and tool metrics.
pub mod observability;
/// MCP permission validation.
pub mod permissions;
/// Connector provider registry.
pub mod providers;
/// Sandboxed command execution.
pub mod sandbox;
/// Tool argument/schema validation.
pub mod schema;
/// Security helpers (paths, hashes, validation).
pub mod security;
/// Stdio JSON-RPC MCP server.
pub mod server;
/// Skill gateway.
pub mod skills;
/// Server-Sent Events sessions for the remote transport.
pub mod sse;
/// Cross-process advisory locking for JSON-backed project stores.
pub mod store_lock;
/// MCP task store.
pub mod tasks;
/// TLS configuration for the remote transport.
pub mod tls;
/// MCP trust and approval policy.
pub mod trust;
/// Persistent trust store.
pub mod trust_store;
/// MCP workspace access.
pub mod workspace;

pub use audit::{audit_allow, audit_circuit_open, audit_deny, audit_secret_deny};
pub use auth::{bearer_token, load_api_key, verify_token};
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerMcpClient};
pub use cli_trust::{block_mcp, revoke_mcp, trust_mcp};
pub use community_registry::{
    CommunityMcpManifest, CommunityMcpRegistryClient, CommunityRegistryIndex,
};
pub use composio::ComposioProvider;
pub use composio_auth::{AuthLink, ComposioAuth, ConnectedAccount};
pub use composio_registry::{ComposioAccount, ComposioRegistry};
pub use config::{build_http_client, ResourceLimits};
pub use connectors::{AuthMethod, Connector, ConnectorsMcp};
pub use context::{load_context, WorkspaceContext};
pub use custom_mcp::{
    CustomMcpRegistry, CustomMcpServerConfig, CustomMcpStore, McpTransport, StdioMcpClient,
    StreamableHttpMcpClient,
};
pub use dispatcher::{
    DispatchResult, McpDispatcher, SessionLifecycle, SessionState, MCP_PROTOCOL_VERSION,
    SERVER_NOT_INITIALIZED_CODE, SUPPORTED_PROTOCOL_VERSIONS,
};
pub use error::McpAuthorizationError;
pub use execution_gate::{authorize as authorize_mcp_execution, McpExecutionRequest};
pub use github::{resolve_repo_target, GithubProvider, RepoTarget};
pub use global_mcp::{
    GlobalMcpEntry, GlobalMcpRegistry, GlobalMcpStore, ProjectMcpReferences, ProjectMcpRefs,
};
pub use http::{build_router, serve, AppState, HttpServerConfig};
pub use memory::{MemoryEntry, MemoryMcp, MemoryScope};
pub use observability::{
    client_name_version, McpEvent, McpHook, McpHooks, ToolMetrics, ToolMetricsSnapshot, MAX_HOOKS,
    MAX_TRACKED_TOOLS,
};
pub use permissions::{
    is_blocked_environment, is_valid_env_name, require as require_permission, McpPermissions,
    Permission,
};
pub use providers::{
    ConnectorProvider, CustomMcpProvider, GatewayProvider, ProviderRegistry, ToolCallResult,
    ToolContent, ToolDescriptor, UnconfiguredProvider,
};
/// Linux-only: the injected-`bwrap` variant used to test fail-closed
/// behavior without mutating the process environment.
#[cfg(target_os = "linux")]
pub use sandbox::wrap_command_with;
pub use sandbox::{sandbox_available, wrap_command, SandboxConfig, SandboxLimits};
pub use schema::validate_tool_arguments;
pub use security::{
    atomic_write, secure_destination, secure_path, sha256_file, validate_command, validate_id,
    validate_url, verify_sha256, PackageIntegrity,
};
pub use server::StdioMcpServer;
pub use skills::SkillMcp;
pub use sse::{Session, SessionRegistry, SseEvent};
pub use store_lock::StoreLock;
pub use tasks::{Task, TaskPriority, TaskStatus, TasksMcp};
pub use tls::TlsConfig;
pub use trust::{can_enable, McpApproval, TrustLevel, TrustStore};
pub use trust_store::PersistentTrustStore;
pub use workspace::{WorkspaceFile, WorkspaceMcp};
