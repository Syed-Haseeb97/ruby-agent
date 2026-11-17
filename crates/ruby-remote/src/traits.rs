//! ChannelAdapter and RemoteCommandHandler traits for multi-platform remote control.

use crate::types::{IncomingMessage, RemoteError};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait RemoteCommandHandler: Send + Sync {
    /// Dispatches an incoming remote message, executing commands or creating tasks.
    async fn handle_command(&self, message: IncomingMessage) -> Result<String, RemoteError>;
}

#[async_trait]
pub trait ChannelAdapter: Send + Sync {
    /// Identifies the protocol name (e.g. "telegram", "discord").
    fn name(&self) -> &str;

    /// Starts long polling or inbound message stream.
    async fn start_polling(
        &self,
        handler: Arc<dyn RemoteCommandHandler>,
    ) -> Result<(), RemoteError>;

    /// Sends a text reply to a specific chat id.
    async fn send_reply(&self, chat_id: i64, text: &str) -> Result<(), RemoteError>;

    /// Sends a binary file attachment (stub for R1/R2 screenshots and logs).
    async fn send_file(
        &self,
        chat_id: i64,
        filename: &str,
        data: &[u8],
        caption: Option<&str>,
    ) -> Result<(), RemoteError>;

    /// Stops long polling cleanly.
    async fn stop(&self) -> Result<(), RemoteError>;

    /// Returns true if currently connected and polling.
    fn is_connected(&self) -> bool;
}

/// No-op command handler for default setups.
pub struct NoopCommandHandler;

#[async_trait]
impl RemoteCommandHandler for NoopCommandHandler {
    async fn handle_command(&self, _message: IncomingMessage) -> Result<String, RemoteError> {
        Ok("received".to_string())
    }
}
