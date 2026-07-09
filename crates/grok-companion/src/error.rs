use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompanionError {
    #[error("{0}")]
    Message(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("grok failed: {0}")]
    Grok(String),
}

pub type Result<T> = std::result::Result<T, CompanionError>;

impl CompanionError {
    pub fn msg(s: impl Into<String>) -> Self {
        Self::Message(s.into())
    }
}
