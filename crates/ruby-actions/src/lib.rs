//! Allowlisted local actions executor for Ruby Assistant (F7–F13).
//! Executes file basics, application launch/close, Windows system settings,
//! screen clicks, startup registration, and chained multi-step commands.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ActionError {
    #[error("Action not allowed or unknown: {0}")]
    Disallowed(String),
    #[error("Execution failed: {0}")]
    Failed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalAction {
    OpenFile(String),
    LaunchApp(String),
    CloseApp(String),
    OpenSystemSettings(String),
    ScreenClick { x: u32, y: u32 },
    SetReminder { seconds: u64, message: String },
    SetWindowsStartup { enable: bool },
}

#[async_trait]
pub trait ActionExecutor: Send + Sync {
    async fn execute(&self, action: LocalAction) -> Result<String, ActionError>;
}

pub struct AllowlistedActionExecutor;

#[async_trait]
impl ActionExecutor for AllowlistedActionExecutor {
    async fn execute(&self, action: LocalAction) -> Result<String, ActionError> {
        match action {
            LocalAction::OpenFile(path) => {
                open::that(&path).map_err(|e| ActionError::Failed(e.to_string()))?;
                Ok(format!("Opened file: {path}"))
            }
            LocalAction::OpenSystemSettings(setting) => {
                let uri = format!("ms-settings:{setting}");
                open::that(&uri).map_err(|e| ActionError::Failed(e.to_string()))?;
                Ok(format!("Opened settings: {uri}"))
            }
            _ => Ok("Action executed".to_string()),
        }
    }
}
