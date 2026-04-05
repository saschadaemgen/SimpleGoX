//! Error types for SimpleGoX core.

use thiserror::Error;

/// Top-level error type for sgx-core operations.
#[derive(Debug, Error)]
pub enum SgxError {
    #[error("Matrix SDK error: {0}")]
    Matrix(#[from] matrix_sdk::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] matrix_sdk::HttpError),

    #[error("Client build error: {0}")]
    ClientBuild(#[from] matrix_sdk::ClientBuildError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Cryptography error: {0}")]
    Crypto(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
