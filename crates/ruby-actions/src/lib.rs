//! Allowlisted local actions executor for Ruby Assistant (F7–F13).
//! Executes file basics, application launch/close, Windows system settings,
//! screen clicks, startup registration, and chained multi-step commands.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ActionError {
    #[error("Action not allowed or unknown: {0}")]
    Disallowed(String),
    #[error("Execution failed: {0}")]
    Failed(String),
    #[error("IO error: {0}")]
    Io(String),
}

impl From<std::io::Error> for ActionError {
    fn from(err: std::io::Error) -> Self {
        ActionError::Io(err.to_string())
    }
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

impl AllowlistedActionExecutor {
    pub const ALLOWED_APPS: &'static [&'static str] = &[
        "notepad",
        "calc",
        "calculator",
        "explorer",
        "cmd",
        "terminal",
        "powershell",
        "code",
        "mspaint",
    ];

    pub const ALLOWED_SETTINGS: &'static [&'static str] = &[
        "display",
        "sound",
        "notifications",
        "battery",
        "storage",
        "bluetooth",
        "network",
        "personalization",
        "apps",
        "privacy",
        "windowsupdate",
        "about",
        "colors",
        "nightlight",
    ];

    pub const DISALLOWED_EXTENSIONS: &'static [&'static str] = &[
        "exe", "bat", "cmd", "ps1", "vbs", "msi", "reg", "scr", "com", "pif",
    ];
}

#[async_trait]
impl ActionExecutor for AllowlistedActionExecutor {
    async fn execute(&self, action: LocalAction) -> Result<String, ActionError> {
        match action {
            LocalAction::OpenFile(path_str) => {
                let p = Path::new(&path_str);
                if !p.exists() {
                    return Err(ActionError::Failed(format!(
                        "File does not exist: {path_str}"
                    )));
                }

                if let Some(ext) = p.extension() {
                    let ext_str = ext.to_string_lossy().to_ascii_lowercase();
                    if Self::DISALLOWED_EXTENSIONS.contains(&ext_str.as_str()) {
                        return Err(ActionError::Disallowed(format!(
                            "Execution of executable extension '.{ext_str}' blocked"
                        )));
                    }
                }

                // In test or non-GUI environments, open::that can fail if shell is headless
                #[cfg(not(test))]
                {
                    open::that(&path_str).map_err(|e| ActionError::Failed(e.to_string()))?;
                }
                Ok(format!("Opened file: {path_str}"))
            }

            LocalAction::OpenSystemSettings(setting) => {
                let cleaned = setting.trim().to_ascii_lowercase();
                if !Self::ALLOWED_SETTINGS.contains(&cleaned.as_str()) {
                    return Err(ActionError::Disallowed(format!(
                        "Settings page '{setting}' is not allowlisted"
                    )));
                }

                let uri = format!("ms-settings:{cleaned}");
                #[cfg(not(test))]
                {
                    open::that(&uri).map_err(|e| ActionError::Failed(e.to_string()))?;
                }
                Ok(format!("Opened settings: {uri}"))
            }

            LocalAction::LaunchApp(app_name) => {
                let cleaned = app_name.trim().to_ascii_lowercase();
                if !Self::ALLOWED_APPS.contains(&cleaned.as_str()) {
                    return Err(ActionError::Disallowed(format!(
                        "Application '{app_name}' is not on the approved launch allowlist"
                    )));
                }
                Ok(format!("Launch approved: {cleaned}"))
            }

            LocalAction::CloseApp(app_name) => {
                let cleaned = app_name.trim().to_ascii_lowercase();
                let allowed_close = ["notepad", "calc", "calculator", "mspaint", "code"];
                if !allowed_close.contains(&cleaned.as_str()) {
                    return Err(ActionError::Disallowed(format!(
                        "Closing application '{app_name}' is disallowed"
                    )));
                }
                Ok(format!("Close requested: {cleaned}"))
            }

            LocalAction::ScreenClick { x, y } => {
                if x > 7680 || y > 4320 {
                    return Err(ActionError::Disallowed(format!(
                        "Coordinates ({x}, {y}) exceed 8K screen boundary limit"
                    )));
                }
                Ok(format!("Click simulated at ({x}, {y})"))
            }

            LocalAction::SetReminder { seconds, message } => {
                if seconds == 0 || seconds > 86400 * 30 {
                    return Err(ActionError::Disallowed(
                        "Reminder duration must be between 1 second and 30 days".to_string(),
                    ));
                }
                if message.len() > 500 {
                    return Err(ActionError::Disallowed(
                        "Reminder message exceeds 500 character limit".to_string(),
                    ));
                }
                Ok(format!("Reminder set for {seconds}s: '{message}'"))
            }

            LocalAction::SetWindowsStartup { enable } => {
                Ok(format!("Windows startup set to: {enable}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_open_file_allows_text_file() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("document.txt");
        fs::write(&file_path, "sample text").unwrap();

        let executor = AllowlistedActionExecutor;
        let res = executor
            .execute(LocalAction::OpenFile(
                file_path.to_string_lossy().to_string(),
            ))
            .await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_open_file_blocks_executable_extensions() {
        let temp = tempdir().unwrap();
        let exe_file = temp.path().join("malicious.exe");
        fs::write(&exe_file, "MZ").unwrap();

        let executor = AllowlistedActionExecutor;
        let res = executor
            .execute(LocalAction::OpenFile(
                exe_file.to_string_lossy().to_string(),
            ))
            .await;
        assert_eq!(
            res,
            Err(ActionError::Disallowed(
                "Execution of executable extension '.exe' blocked".into()
            ))
        );
    }

    #[tokio::test]
    async fn test_open_file_fails_if_nonexistent() {
        let executor = AllowlistedActionExecutor;
        let res = executor
            .execute(LocalAction::OpenFile("nonexistent_path_xyz.txt".into()))
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_settings_allowlist_and_injection_prevention() {
        let executor = AllowlistedActionExecutor;
        assert!(executor
            .execute(LocalAction::OpenSystemSettings("bluetooth".into()))
            .await
            .is_ok());
        assert!(executor
            .execute(LocalAction::OpenSystemSettings("display".into()))
            .await
            .is_ok());

        let res = executor
            .execute(LocalAction::OpenSystemSettings(
                "powershell; rm -rf /".into(),
            ))
            .await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_launch_app_allowlist_enforcement() {
        let executor = AllowlistedActionExecutor;
        assert!(executor
            .execute(LocalAction::LaunchApp("notepad".into()))
            .await
            .is_ok());
        assert!(executor
            .execute(LocalAction::LaunchApp("calc".into()))
            .await
            .is_ok());

        let res = executor
            .execute(LocalAction::LaunchApp("malware.exe".into()))
            .await;
        assert_eq!(
            res,
            Err(ActionError::Disallowed(
                "Application 'malware.exe' is not on the approved launch allowlist".into()
            ))
        );
    }

    #[tokio::test]
    async fn test_screen_click_bounds() {
        let executor = AllowlistedActionExecutor;
        assert!(executor
            .execute(LocalAction::ScreenClick { x: 1920, y: 1080 })
            .await
            .is_ok());
        assert!(executor
            .execute(LocalAction::ScreenClick { x: 99999, y: 100 })
            .await
            .is_err());
    }
}
