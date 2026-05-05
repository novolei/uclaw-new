// ─── Sub-error types ────────────────────────────────────────────────────

/// LLM-specific errors
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("Provider not configured: {0}")]
    ProviderNotConfigured(String),
    #[error("API request failed: {0}")]
    ApiRequestFailed(String),
    #[error("Rate limited, retry after {retry_after_ms}ms")]
    RateLimited { retry_after_ms: u64 },
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Token limit exceeded")]
    TokenLimitExceeded,
}

/// Agent loop errors
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Max iterations reached")]
    MaxIterations,
    #[error("Loop cancelled")]
    Cancelled,
    #[error("Invalid state transition: {from} -> {to}")]
    InvalidStateTransition { from: String, to: String },
}

// ─── Unified application error ──────────────────────────────────────────

/// Unified error type for the entire application.
///
/// Sub-module errors (McpError, ToolError, SkillParseError) remain in
/// their own modules and are converted into `Error` via `From` impls
/// when they cross module boundaries.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // ── Infrastructure ──
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    // ── Domain errors ──
    #[error("LLM error: {0}")]
    Llm(#[from] LlmError),

    #[error("Agent error: {0}")]
    Agent(#[from] AgentError),

    #[error("MCP error: {0}")]
    Mcp(String),

    #[error("Tool error: {0}")]
    Tool(String),

    // ── Application-level ──
    #[error("Config error: {0}")]
    Config(String),

    #[error("Auth error: {0}")]
    Auth(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Type alias for convenience.
pub type AppError = Error;

/// Application-wide result type.
pub type AppResult<T> = Result<T, Error>;

// ─── Conversions from module-local errors ──────────────────────────────

impl From<crate::mcp::McpError> for Error {
    fn from(e: crate::mcp::McpError) -> Self {
        Error::Mcp(e.to_string())
    }
}

impl From<crate::agent::tools::tool::ToolError> for Error {
    fn from(e: crate::agent::tools::tool::ToolError) -> Self {
        Error::Tool(e.to_string())
    }
}

// ─── Serialize for Tauri IPC ───────────────────────────────────────────

impl serde::Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
