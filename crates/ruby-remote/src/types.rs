//! Domain types, message formats, and task definitions for Remote Control.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RemoteError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("Telegram 409 Conflict: Another poller is active")]
    Conflict409,
    #[error("Telegram 429 Rate Limit: retry after {0} seconds")]
    RateLimited429(u64),
    #[error("Telegram 401 Unauthorized: Token revoked or invalid")]
    TokenRevoked,
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Message is stale: {0}")]
    StaleMessage(String),
    #[error("Unauthorized sender ID: {0}")]
    UnauthorizedSender(i64),
    #[error("Invalid confirmation code: {0}")]
    InvalidCode(String),
    #[error("Action was cancelled: {0}")]
    Cancelled(String),
    #[error("I/O or Persistence error: {0}")]
    Io(String),
}

/// Task state machine for remote requests:
/// Queued -> Running -> NeedsInput -> Done | Failed | Cancelled
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Queued,
    Running,
    NeedsInput,
    Done,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Queued => "queued",
            TaskState::Running => "running",
            TaskState::NeedsInput => "needs_input",
            TaskState::Done => "done",
            TaskState::Failed => "failed",
            TaskState::Cancelled => "cancelled",
        }
    }
}

/// Record tracking a remote task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTask {
    pub id: String,
    pub sender_id: i64,
    pub raw_prompt: String,
    pub state: TaskState,
    pub created_at_secs: u64,
    pub updated_at_secs: u64,
    pub result: Option<String>,
    pub error: Option<String>,
}

/// Normalized incoming message from a chat platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    pub update_id: i64,
    pub message_id: i64,
    pub sender_id: i64,
    pub chat_id: i64,
    pub timestamp_secs: u64,
    pub text: String,
    pub is_private: bool,
}

/// Overall status of the remote subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteStatus {
    pub enabled: bool,
    pub paired: bool,
    pub paired_user_id: Option<i64>,
    pub connected: bool,
    pub active_pairing_code: Option<String>,
    pub pairing_code_expires_in_secs: Option<u64>,
    pub last_command_time_secs: Option<u64>,
}
