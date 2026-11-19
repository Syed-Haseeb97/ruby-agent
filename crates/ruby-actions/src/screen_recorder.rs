//! Video-only screen recording service (Feature F5 / M6 Parity).
//! Enforces:
//! 1. Explicit start confirmation via UnifiedConfirmationManager.
//! 2. Video-only capture (strictly audio-disabled with `-an`).
//! 3. Output directory outside the repository (defaulting to `%LOCALAPPDATA%\Ruby\recordings\`).
//! 4. Automatic pause/stop if any window on the security denylist becomes visible.
//! 5. Static, calm indicator state.

use chrono::Utc;
use ruby_policy::computer_policy::is_window_denylisted_with_custom;
use ruby_policy::UnifiedConfirmationManager;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RecordingError {
    #[error("Recording requires confirmation: {0}")]
    RequiresConfirmation(String),
    #[error("Recording error: {0}")]
    Failed(String),
    #[error("Recording paused due to security denylist: window '{0}' is visible")]
    DenylistedWindowVisible(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RecordingState {
    Idle,
    Recording,
    PausedByDenylist,
    Stopped,
}

/// Configuration for screen recording sessions.
#[derive(Debug, Clone)]
pub struct ScreenRecordingConfig {
    pub output_dir: PathBuf,
    pub framerate: u32,
    pub denylist: Vec<String>,
}

impl Default for ScreenRecordingConfig {
    fn default() -> Self {
        let base_dir = if let Ok(custom) = std::env::var("RUBY_DATA_DIR") {
            PathBuf::from(custom)
        } else if let Ok(local) = std::env::var("LOCALAPPDATA") {
            PathBuf::from(local).join("Ruby")
        } else {
            let user = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_else(|_| ".".to_string());
            Path::new(&user).join(".ruby")
        };

        Self {
            output_dir: base_dir.join("recordings"),
            framerate: 30,
            denylist: Vec::new(),
        }
    }
}

type WindowList = Vec<(String, String)>;

/// Service managing video-only desktop recordings with denylist protection.
pub struct ScreenRecorder {
    config: ScreenRecordingConfig,
    state: Arc<Mutex<RecordingState>>,
    active_output_file: Arc<Mutex<Option<PathBuf>>>,
    confirmations: Arc<UnifiedConfirmationManager>,
    visible_windows_override: Arc<Mutex<Option<WindowList>>>,
}

impl ScreenRecorder {
    pub fn new(
        config: ScreenRecordingConfig,
        confirmations: Arc<UnifiedConfirmationManager>,
    ) -> Self {
        let _ = std::fs::create_dir_all(&config.output_dir);
        Self {
            config,
            state: Arc::new(Mutex::new(RecordingState::Idle)),
            active_output_file: Arc::new(Mutex::new(None)),
            confirmations,
            visible_windows_override: Arc::new(Mutex::new(None)),
        }
    }

    pub fn state(&self) -> RecordingState {
        *self.state.lock().unwrap()
    }

    pub fn set_visible_windows_override(&self, windows: Vec<(String, String)>) {
        *self.visible_windows_override.lock().unwrap() = Some(windows);
    }

    pub fn clear_visible_windows_override(&self) {
        *self.visible_windows_override.lock().unwrap() = None;
    }

    fn inspect_visible_windows(&self) -> Vec<(String, String)> {
        if let Some(list) = self.visible_windows_override.lock().unwrap().clone() {
            return list;
        }
        crate::computer_use::get_all_visible_windows_info()
    }

    /// Builds the hardened ffmpeg argument vector: strictly video-only (-an).
    pub fn build_ffmpeg_args(framerate: u32, output_path: &Path) -> Vec<String> {
        vec![
            "-y".to_string(),
            "-f".to_string(),
            "gdigrab".to_string(),
            "-framerate".to_string(),
            framerate.to_string(),
            "-i".to_string(),
            "desktop".to_string(),
            "-an".to_string(), // Strictly disable audio capture
            "-c:v".to_string(),
            "libx264".to_string(),
            "-pix_fmt".to_string(),
            "yuv420p".to_string(),
            output_path.to_string_lossy().to_string(),
        ]
    }

    /// Requests start of screen recording, returning a confirmation code if unconfirmed.
    pub fn request_start(&self) -> String {
        let desc = "Start full-desktop video recording (video-only)";
        self.confirmations.register_action(
            "recorder",
            "start_recording",
            serde_json::json!({}),
            desc,
        )
    }

    /// Starts recording after verifying confirmation code and asserting no denylisted windows.
    pub fn start_with_confirmation(&self, code: &str) -> Result<PathBuf, RecordingError> {
        self.confirmations
            .verify_and_consume(code)
            .map_err(|e| RecordingError::RequiresConfirmation(e.to_string()))?;

        // 1. Check all visible windows against security denylist
        let visible = self.inspect_visible_windows();
        for (title, proc_name) in visible {
            if is_window_denylisted_with_custom(&title, &proc_name, &self.config.denylist) {
                *self.state.lock().unwrap() = RecordingState::PausedByDenylist;
                return Err(RecordingError::DenylistedWindowVisible(title));
            }
        }

        // 2. Prepare output path outside repo
        let filename = format!("recording_{}.mp4", Utc::now().format("%Y%m%d_%H%M%S"));
        let output_path = self.config.output_dir.join(filename);

        *self.active_output_file.lock().unwrap() = Some(output_path.clone());
        *self.state.lock().unwrap() = RecordingState::Recording;

        Ok(output_path)
    }

    /// Periodic safety heartbeat: checks if any denylisted window became visible.
    /// Pauses recording immediately if detected.
    pub fn check_security_denylist_tick(&self) -> Result<(), RecordingError> {
        if *self.state.lock().unwrap() != RecordingState::Recording {
            return Ok(());
        }

        let visible = self.inspect_visible_windows();
        for (title, proc_name) in visible {
            if is_window_denylisted_with_custom(&title, &proc_name, &self.config.denylist) {
                *self.state.lock().unwrap() = RecordingState::PausedByDenylist;
                return Err(RecordingError::DenylistedWindowVisible(title));
            }
        }

        Ok(())
    }

    /// Stops the active recording session.
    pub fn stop(&self) -> Result<Option<PathBuf>, RecordingError> {
        let prev = *self.state.lock().unwrap();
        *self.state.lock().unwrap() = RecordingState::Stopped;

        let path = self.active_output_file.lock().unwrap().take();
        if prev == RecordingState::Recording || prev == RecordingState::PausedByDenylist {
            Ok(path)
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_ffmpeg_args_strictly_disable_audio() {
        let path = Path::new("test.mp4");
        let args = ScreenRecorder::build_ffmpeg_args(30, path);
        // -an must be present to ensure strictly video-only capture
        assert!(args.contains(&"-an".to_string()));
        assert!(args.contains(&"desktop".to_string()));
        assert!(args.contains(&"gdigrab".to_string()));
    }

    #[test]
    fn test_screen_recording_confirmation_flow_and_denylist_guard() {
        let temp = tempdir().unwrap();
        let config = ScreenRecordingConfig {
            output_dir: temp.path().join("recordings"),
            framerate: 30,
            denylist: vec!["CustomBank".into()],
        };

        let confirmations = Arc::new(UnifiedConfirmationManager::new());
        let recorder = ScreenRecorder::new(config, confirmations);
        assert_eq!(recorder.state(), RecordingState::Idle);

        // 1. Invalid confirmation fails
        let err = recorder.start_with_confirmation("0000");
        assert!(matches!(err, Err(RecordingError::RequiresConfirmation(_))));

        // 2. Request start produces 4-digit code
        let code = recorder.request_start();
        assert_eq!(code.len(), 4);

        // 3. Denylisted window prevents start even with valid code
        recorder.set_visible_windows_override(vec![
            ("Visual Studio Code".into(), "code.exe".into()),
            ("1Password - Vault".into(), "1password.exe".into()),
        ]);
        let err_denied = recorder.start_with_confirmation(&code);
        assert!(matches!(
            err_denied,
            Err(RecordingError::DenylistedWindowVisible(_))
        ));
        assert_eq!(recorder.state(), RecordingState::PausedByDenylist);

        // 4. Clean window list allows successful start with new code
        let code2 = recorder.request_start();
        recorder.set_visible_windows_override(vec![
            ("Visual Studio Code".into(), "code.exe".into()),
            ("Notepad".into(), "notepad.exe".into()),
        ]);
        let output_path = recorder.start_with_confirmation(&code2).unwrap();
        assert_eq!(recorder.state(), RecordingState::Recording);
        assert!(output_path.to_string_lossy().contains("recordings"));

        // 5. Periodic tick catches new denylisted window (CustomBank) and auto-pauses
        recorder.set_visible_windows_override(vec![
            ("Visual Studio Code".into(), "code.exe".into()),
            ("CustomBank Online Portal".into(), "custombank.exe".into()),
        ]);
        let tick_res = recorder.check_security_denylist_tick();
        assert!(tick_res.is_err());
        assert_eq!(recorder.state(), RecordingState::PausedByDenylist);

        // 6. Stop finalizes session
        let stopped_path = recorder.stop().unwrap();
        assert_eq!(stopped_path, Some(output_path));
        assert_eq!(recorder.state(), RecordingState::Stopped);
    }
}
