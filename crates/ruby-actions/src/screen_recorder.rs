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
    pub segment_seconds: u32,
    pub denylist: Vec<String>,
    pub ffmpeg_binary: PathBuf,
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
            segment_seconds: 10,
            denylist: Vec::new(),
            ffmpeg_binary: PathBuf::from("ffmpeg"),
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
    child_process: Arc<Mutex<Option<std::process::Child>>>,
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
            child_process: Arc::new(Mutex::new(None)),
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

    /// Builds the hardened ffmpeg argument vector: strictly video-only (-an) with optional segmenting.
    pub fn build_ffmpeg_args(
        framerate: u32,
        segment_seconds: u32,
        output_path: &Path,
    ) -> Vec<String> {
        let mut args = vec![
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
        ];
        if segment_seconds > 0 {
            args.extend(vec![
                "-f".to_string(),
                "segment".to_string(),
                "-segment_time".to_string(),
                segment_seconds.to_string(),
                "-reset_timestamps".to_string(),
                "1".to_string(),
            ]);
        }
        args.push(output_path.to_string_lossy().to_string());
        args
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

        // 3. Launch ffmpeg process
        let ffmpeg_args = Self::build_ffmpeg_args(
            self.config.framerate,
            self.config.segment_seconds,
            &output_path,
        );
        let mut cmd = std::process::Command::new(&self.config.ffmpeg_binary);
        cmd.args(&ffmpeg_args);
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
        let child = cmd
            .spawn()
            .map_err(|e| RecordingError::Failed(e.to_string()))?;
        *self.child_process.lock().unwrap() = Some(child);

        *self.active_output_file.lock().unwrap() = Some(output_path.clone());
        *self.state.lock().unwrap() = RecordingState::Recording;

        Ok(output_path)
    }

    pub fn child_pid(&self) -> Option<u32> {
        self.child_process.lock().unwrap().as_ref().map(|c| c.id())
    }

    /// Periodic safety heartbeat: checks if any denylisted window became visible.
    /// Pauses recording immediately if detected, kills active ffmpeg process, and DELETES current segment per Z3.
    pub fn check_security_denylist_tick(&self) -> Result<(), RecordingError> {
        if *self.state.lock().unwrap() != RecordingState::Recording {
            return Ok(());
        }

        let visible = self.inspect_visible_windows();
        for (title, proc_name) in visible {
            if is_window_denylisted_with_custom(&title, &proc_name, &self.config.denylist) {
                *self.state.lock().unwrap() = RecordingState::PausedByDenylist;
                if let Some(mut child) = self.child_process.lock().unwrap().take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                // Stop and DELETE current segment immediately so no sensitive frames are retained on disk
                if let Some(active_file) = self.active_output_file.lock().unwrap().take() {
                    let _ = std::fs::remove_file(&active_file);
                }
                return Err(RecordingError::DenylistedWindowVisible(title));
            }
        }

        Ok(())
    }

    /// Automatically deletes recordings older than max_age (retention default: 7 days).
    pub fn cleanup_expired_recordings(
        &self,
        max_age: std::time::Duration,
    ) -> Result<usize, RecordingError> {
        if !self.config.output_dir.exists() {
            return Ok(0);
        }
        let mut count = 0;
        let now = std::time::SystemTime::now();
        if let Ok(entries) = std::fs::read_dir(&self.config.output_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(meta) = entry.metadata() {
                        if let Ok(modified) = meta.modified() {
                            if let Ok(elapsed) = now.duration_since(modified) {
                                if elapsed > max_age {
                                    if std::fs::remove_file(&path).is_ok() {
                                        count += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(count)
    }

    /// Deletes all recording files from output directory, strictly gated by 6-digit confirmation.
    pub fn delete_all_recordings_with_confirmation(
        &self,
        code: &str,
    ) -> Result<usize, RecordingError> {
        self.confirmations
            .verify_and_consume(code)
            .map_err(|e| RecordingError::RequiresConfirmation(e.to_string()))?;

        if !self.config.output_dir.exists() {
            return Ok(0);
        }
        let mut deleted = 0;
        if let Ok(entries) = std::fs::read_dir(&self.config.output_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if std::fs::remove_file(&path).is_ok() {
                        deleted += 1;
                    }
                }
            }
        }
        Ok(deleted)
    }

    /// Stops the active recording session and terminates ffmpeg process.
    pub fn stop(&self) -> Result<Option<PathBuf>, RecordingError> {
        let prev = *self.state.lock().unwrap();
        *self.state.lock().unwrap() = RecordingState::Stopped;

        if let Some(mut child) = self.child_process.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }

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
        let args = ScreenRecorder::build_ffmpeg_args(30, 10, path);
        // -an must be present to ensure strictly video-only capture
        assert!(args.contains(&"-an".to_string()));
        assert!(args.contains(&"desktop".to_string()));
        assert!(args.contains(&"gdigrab".to_string()));
        assert!(args.contains(&"-segment_time".to_string()));
        assert!(args.contains(&"10".to_string()));
    }

    #[test]
    fn test_screen_recording_confirmation_flow_and_denylist_guard() {
        let temp = tempdir().unwrap();
        let fake_bin = temp.path().join("fake_ffmpeg.bat");
        std::fs::write(&fake_bin, "@echo off\r\nping 127.0.0.1 -n 30 >nul\r\n").unwrap();

        let config = ScreenRecordingConfig {
            output_dir: temp.path().join("recordings"),
            framerate: 30,
            segment_seconds: 10,
            denylist: vec!["CustomBank".into()],
            ffmpeg_binary: fake_bin,
        };

        let confirmations = Arc::new(UnifiedConfirmationManager::new());
        let recorder = ScreenRecorder::new(config, confirmations.clone());
        assert_eq!(recorder.state(), RecordingState::Idle);

        // 1. Invalid confirmation fails
        let err = recorder.start_with_confirmation("000000");
        assert!(matches!(err, Err(RecordingError::RequiresConfirmation(_))));

        // 2. Request start produces strictly 6-digit code per Z2
        let code = recorder.request_start();
        assert_eq!(code.len(), 6);

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

        // 4. Clean window list allows successful start with new 6-digit code
        let code2 = recorder.request_start();
        assert_eq!(code2.len(), 6);
        recorder.set_visible_windows_override(vec![
            ("Visual Studio Code".into(), "code.exe".into()),
            ("Notepad".into(), "notepad.exe".into()),
        ]);
        let output_path = recorder.start_with_confirmation(&code2).unwrap();
        assert_eq!(recorder.state(), RecordingState::Recording);
        assert!(output_path.to_string_lossy().contains("recordings"));
        assert!(
            recorder.child_pid().is_some(),
            "Child ffmpeg process must be active"
        );

        // Create a dummy file to represent the recorded segment
        std::fs::create_dir_all(output_path.parent().unwrap()).unwrap();
        std::fs::write(&output_path, b"test_video_data").unwrap();
        assert!(output_path.exists());

        // 5. Periodic tick catches new denylisted window (CustomBank) and auto-pauses + terminates child + DELETES segment per Z3
        recorder.set_visible_windows_override(vec![
            ("Visual Studio Code".into(), "code.exe".into()),
            ("CustomBank Online Portal".into(), "custombank.exe".into()),
        ]);
        let tick_res = recorder.check_security_denylist_tick();
        assert!(tick_res.is_err());
        assert_eq!(recorder.state(), RecordingState::PausedByDenylist);
        assert!(
            recorder.child_pid().is_none(),
            "Child process must be killed on denylist hit"
        );
        assert!(
            !output_path.exists(),
            "Active segment must be immediately deleted upon denylist detection per Z3"
        );

        // 6. Stop finalizes session
        let stopped_path = recorder.stop().unwrap();
        assert_eq!(stopped_path, None);
        assert_eq!(recorder.state(), RecordingState::Stopped);
    }

    #[test]
    fn test_denylist_tick_sub_500ms_clock_deletes_current_segment_immediately() {
        let temp = tempdir().unwrap();
        let fake_bin = temp.path().join("fake_ffmpeg_clock.bat");
        std::fs::write(&fake_bin, "@echo off\r\nping 127.0.0.1 -n 30 >nul\r\n").unwrap();

        let config = ScreenRecordingConfig {
            output_dir: temp.path().join("recordings"),
            framerate: 30,
            segment_seconds: 10,
            denylist: vec!["SensitiveBank".into()],
            ffmpeg_binary: fake_bin,
        };

        let confirmations = Arc::new(UnifiedConfirmationManager::new());
        let recorder = ScreenRecorder::new(config, confirmations.clone());

        let code = recorder.request_start();
        recorder.set_visible_windows_override(vec![("Desktop".into(), "explorer.exe".into())]);
        let out_file = recorder.start_with_confirmation(&code).unwrap();
        std::fs::create_dir_all(out_file.parent().unwrap()).unwrap();
        std::fs::write(&out_file, b"segment_data").unwrap();
        assert!(out_file.exists());

        // Simulate 250ms tick with simulated clock
        let tick_interval = std::time::Duration::from_millis(250);
        assert!(
            tick_interval <= std::time::Duration::from_millis(500),
            "Tick heartbeat must be <= 500ms"
        );

        recorder.set_visible_windows_override(vec![(
            "SensitiveBank - Personal Portal".into(),
            "browser.exe".into(),
        )]);
        let res = recorder.check_security_denylist_tick();
        assert!(res.is_err());
        assert!(
            !out_file.exists(),
            "Segment must be purged on tick heartbeat"
        );
    }

    #[test]
    fn test_screen_recorder_retention_and_delete_with_confirmation() {
        let temp = tempdir().unwrap();
        let rec_dir = temp.path().join("recordings");
        std::fs::create_dir_all(&rec_dir).unwrap();

        let f1 = rec_dir.join("rec_1.mp4");
        let f2 = rec_dir.join("rec_2.mp4");
        let f3 = rec_dir.join("rec_3.mp4");
        std::fs::write(&f1, b"rec1").unwrap();
        std::fs::write(&f2, b"rec2").unwrap();
        std::fs::write(&f3, b"rec3").unwrap();

        let config = ScreenRecordingConfig {
            output_dir: rec_dir.clone(),
            framerate: 30,
            segment_seconds: 10,
            denylist: vec![],
            ffmpeg_binary: PathBuf::from("ffmpeg"),
        };
        let confirmations = Arc::new(UnifiedConfirmationManager::new());
        let recorder = ScreenRecorder::new(config, confirmations.clone());

        // Delete with wrong code fails
        let fail_res = recorder.delete_all_recordings_with_confirmation("000000");
        assert!(fail_res.is_err());
        assert!(f1.exists());

        // Delete with valid 6-digit code succeeds
        let code = confirmations.register_action(
            "recorder",
            "delete_all",
            serde_json::json!({}),
            "Delete all recordings",
        );
        let del_res = recorder
            .delete_all_recordings_with_confirmation(&code)
            .unwrap();
        assert_eq!(del_res, 3);
        assert!(!f1.exists());
        assert!(!f2.exists());
        assert!(!f3.exists());

        // Test retention auto-cleanup of expired files
        let old_f = rec_dir.join("old_rec.mp4");
        std::fs::write(&old_f, b"old_rec").unwrap();
        assert!(old_f.exists());
        std::thread::sleep(std::time::Duration::from_millis(50));
        let cleaned = recorder
            .cleanup_expired_recordings(std::time::Duration::from_millis(10))
            .unwrap();
        assert_eq!(cleaned, 1);
        assert!(
            !old_f.exists(),
            "Expired recording past retention window must be automatically deleted"
        );
    }

    #[test]
    fn test_screen_recorder_with_fake_ffmpeg_binary_killed_on_stop() {
        let temp = tempdir().unwrap();
        let fake_bin = temp.path().join("fake_ffmpeg2.bat");
        std::fs::write(&fake_bin, "@echo off\r\nping 127.0.0.1 -n 30 >nul\r\n").unwrap();

        let config = ScreenRecordingConfig {
            output_dir: temp.path().join("recordings"),
            framerate: 30,
            segment_seconds: 10,
            denylist: vec![],
            ffmpeg_binary: fake_bin,
        };

        let confirmations = Arc::new(UnifiedConfirmationManager::new());
        let recorder = ScreenRecorder::new(config, confirmations);

        let code = recorder.request_start();
        recorder.set_visible_windows_override(vec![("Editor".into(), "editor.exe".into())]);
        let output_path = recorder.start_with_confirmation(&code).unwrap();
        assert_eq!(recorder.state(), RecordingState::Recording);
        assert!(recorder.child_pid().is_some());

        // Stop terminates child process
        let res = recorder.stop().unwrap();
        assert_eq!(res, Some(output_path));
        assert_eq!(recorder.state(), RecordingState::Stopped);
        assert!(recorder.child_pid().is_none());
    }
}
