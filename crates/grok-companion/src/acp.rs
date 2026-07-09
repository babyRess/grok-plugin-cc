//! Minimal ACP (Agent Client Protocol) notes for future `grok agent stdio` integration.
//!
//! v0.1 still uses headless `grok -p` (JSON) for reliability and sessionId capture.
//! A full ACP client would:
//!   1. spawn `grok agent stdio`
//!   2. JSON-RPC `initialize` / `session/new` / `session/prompt`
//!   3. stream `session/update` until end
//!   4. handle permission requests
//!
//! This module documents the intended surface without blocking the CLI.

/// Runtime selection for companion runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuntimeKind {
    /// Headless `grok -p` (default, production).
    #[default]
    Headless,
    /// Future: `grok agent stdio` ACP client.
    Acp,
}

impl RuntimeKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "headless" | "p" | "cli" => Some(Self::Headless),
            "acp" | "agent" | "stdio" => Some(Self::Acp),
            _ => None,
        }
    }
}

/// Placeholder until ACP client is implemented.
pub fn acp_not_implemented_message() -> &'static str {
    "ACP runtime is not implemented yet in v0.1. Use default headless `grok -p` \
     (sessionId is already captured via --output-format json). Tracked on the roadmap."
}
