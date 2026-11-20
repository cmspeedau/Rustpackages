//! Error types for mpkg-core.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, MpkgError>;

#[derive(Debug, Error)]
pub enum MpkgError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("package '{0}' is already installed")]
    AlreadyInstalled(String),
    #[error("package '{0}' is not installed")]
    NotInstalled(String),
    #[error("history is empty; nothing to roll back")]
    EmptyHistory,
    #[error("verification failed for {0} package(s)")]
    VerifyFailed(usize),
    #[error("network error: {0}")]
    Network(String),
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    Checksum { expected: String, actual: String },
}
