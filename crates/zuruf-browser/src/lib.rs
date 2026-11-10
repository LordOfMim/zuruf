//! Managed Chromium sessions. Raw debugging endpoints never leave this crate.
pub mod capture;
pub mod cdp;
pub mod control;
pub mod extension;
pub mod proxy;
pub mod session;
pub mod types;
pub use session::{Browser, SessionConfig};
pub use types::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("WebSocket: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("CDP: {0}")]
    Cdp(serde_json::Value),
    #[error("browser connection closed")]
    Closed,
    #[error("operation timed out: {0}")]
    Timeout(&'static str),
    #[error("access denied: {0}")]
    Denied(&'static str),
    #[error("invalid data: {0}")]
    Invalid(String),
}
pub type Result<T> = std::result::Result<T, Error>;
