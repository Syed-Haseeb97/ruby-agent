//! Ruby Agent Core Supervisor & Task Lifecycle State Machine.
//! Supports Dual Execution:
//! Mode A: Live Computer Use (Foreground screen driving + SendInput + kill switch).
//! Mode B: Background Task Supervisor (Sandboxed execution, zero cursor hijacking).
//! Enforces: GUI requests in background mode return `GUI_ISOLATION_REQUIRED`.
//! Task states are kept strictly separate from the Orb emotion enum.

pub mod chat;
pub mod tasks;
pub mod tts;
pub mod voice_live;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Task execution failed: {0}")]
    Execution(String),
    #[error("GUI actions in background mode require isolation: GUI_ISOLATION_REQUIRED")]
    GuiIsolationRequired,
    #[error("Task was cancelled by user")]
    Cancelled,
    #[error("Task timed out")]
    Timeout,
}

/// Operational state of the assistant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum OperationalState {
    Idle,
    Listening,
    Thinking,
    Speaking,
    Acting,
    Error,
}

/// Emotional expressions returned by Gemini context analysis.
/// Kept separate from operational states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Emotion {
    Neutral,
    Happy,
    Excited,
    Sad,
    Empathetic,
    Curious,
    Surprised,
}

/// Background Task Lifecycle States.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum TaskStatus {
    Created,
    Queued,
    Running,
    Verifying,
    NeedsInput,
    Completed,
    Failed,
    Cancelled,
    Timeout,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BackgroundTask {
    #[ts(type = "string")]
    pub id: Uuid,
    pub goal: String,
    pub status: TaskStatus,
    #[ts(type = "string")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub output: Option<String>,
}

#[async_trait]
pub trait ExecutionBackend: Send + Sync {
    async fn execute_command(&self, cmd: &str, args: &[String]) -> Result<String, CoreError>;
    async fn write_file(&self, path: &str, contents: &[u8]) -> Result<(), CoreError>;
    async fn read_file(&self, path: &str) -> Result<Vec<u8>, CoreError>;
    async fn request_gui_action(&self, _action: &str) -> Result<(), CoreError> {
        Err(CoreError::GuiIsolationRequired)
    }
}

/// Mock backend for offline CI tests.
pub struct MockBackend;

#[async_trait]
impl ExecutionBackend for MockBackend {
    async fn execute_command(&self, _cmd: &str, _args: &[String]) -> Result<String, CoreError> {
        Ok("Mock command output".to_string())
    }

    async fn write_file(&self, _path: &str, _contents: &[u8]) -> Result<(), CoreError> {
        Ok(())
    }

    async fn read_file(&self, _path: &str) -> Result<Vec<u8>, CoreError> {
        Ok(b"Mock file contents".to_vec())
    }
}

/// The Orb's complete visual state packet crossing the IPC boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OrbVisualState {
    pub operational: OperationalState,
    pub emotion: Emotion,
    pub thinking_spin_active: bool,
    pub speaking_active: bool,
}

impl Default for OrbVisualState {
    fn default() -> Self {
        Self {
            operational: OperationalState::Idle,
            emotion: Emotion::Neutral,
            thinking_spin_active: false,
            speaking_active: false,
        }
    }
}

impl OrbVisualState {
    /// Port of Python `LiquidBlob.uses_black_core`:
    /// Only Idle, Listening, and Speaking operational states use the dark face core.
    /// Sad and all emotional expressions do NOT use the black core.
    pub fn uses_black_core(&self) -> bool {
        match self.operational {
            OperationalState::Idle | OperationalState::Listening | OperationalState::Speaking => {
                // If an emotional expression is actively overriding, black core is turned off
                matches!(self.emotion, Emotion::Neutral)
            }
            OperationalState::Thinking | OperationalState::Error | OperationalState::Acting => {
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sad_mood_has_no_black_core() {
        let state = OrbVisualState {
            operational: OperationalState::Idle,
            emotion: Emotion::Sad,
            ..Default::default()
        };
        assert!(!state.uses_black_core(), "Sad mood must NOT use black core");
    }

    #[test]
    fn test_neutral_listening_and_speaking_keep_approved_core() {
        let idle = OrbVisualState {
            operational: OperationalState::Idle,
            emotion: Emotion::Neutral,
            ..Default::default()
        };
        assert!(idle.uses_black_core());

        let listening = OrbVisualState {
            operational: OperationalState::Listening,
            emotion: Emotion::Neutral,
            ..Default::default()
        };
        assert!(listening.uses_black_core());

        let speaking = OrbVisualState {
            operational: OperationalState::Speaking,
            emotion: Emotion::Neutral,
            ..Default::default()
        };
        assert!(speaking.uses_black_core());
    }

    #[test]
    fn test_emotional_expressions_do_not_use_black_core() {
        for emotion in [
            Emotion::Happy,
            Emotion::Excited,
            Emotion::Empathetic,
            Emotion::Curious,
            Emotion::Surprised,
        ] {
            let state = OrbVisualState {
                operational: OperationalState::Speaking,
                emotion,
                ..Default::default()
            };
            assert!(
                !state.uses_black_core(),
                "{:?} must NOT use black core",
                emotion
            );
        }
    }
}
