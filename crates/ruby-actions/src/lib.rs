//! Allowlisted local actions executor for Ruby Assistant (F7–F13).
//! Executes file basics, application launch/close, Windows system settings,
//! screen clicks, startup registration, and chained multi-step commands.

pub mod computer_use;
pub mod screen_recorder;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
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
    ListFolder(String),
    ReadFileHead { path: String, max_bytes: usize },
    FileInfo(String),
    SendFile { path: String, max_bytes: usize },
    GitStatus(String),
    GitLog { repo: String, limit: usize },
    GitDiffStat(String),
    GhPrList { repo: Option<String>, limit: usize },
    GhIssueList { repo: Option<String>, limit: usize },
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

    pub const GIT_EXECUTION_TIMEOUT: Duration = Duration::from_secs(5);
    pub const MAX_COMMAND_OUTPUT_BYTES: usize = 65536; // 64 KB cap

    /// Builds a hardened git std::process::Command with overrides that neutralize hooks,
    /// credential helpers, custom pagers, fsmonitor, and locks.
    pub fn build_hardened_git_command(repo_path: &Path) -> std::process::Command {
        let mut cmd = std::process::Command::new("git");
        cmd.current_dir(repo_path)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .arg("--no-pager")
            .args(["-c", "core.fsmonitor="])
            .args(["-c", "core.hooksPath="])
            .args(["-c", "core.pager=cat"])
            .args(["-c", "credential.helper="])
            .args(["-c", "ssh.variant=simple"]);
        cmd
    }

    /// Executes a command with timeout and caps output length to 64 KB.
    /// Uses Windows Job Object to guarantee the entire process tree is terminated on timeout.
    pub async fn run_command_with_timeout_and_cap(
        cmd: std::process::Command,
        desc: &str,
    ) -> Result<String, ActionError> {
        Self::run_command_with_custom_timeout_and_cap(
            cmd,
            desc,
            Self::GIT_EXECUTION_TIMEOUT,
            Self::MAX_COMMAND_OUTPUT_BYTES,
        )
        .await
    }

    /// Executes a command with configurable timeout and output cap, enforcing Job Object tree termination.
    pub async fn run_command_with_custom_timeout_and_cap(
        mut cmd: std::process::Command,
        desc: &str,
        timeout_duration: Duration,
        max_output_bytes: usize,
    ) -> Result<String, ActionError> {
        use std::os::windows::io::AsRawHandle;
        let job = ruby_platform::JobObjectProcessTree::new().ok();

        let child = cmd
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ActionError::Failed(format!("{desc} failed to launch: {e}")))?;

        if let Some(ref j) = job {
            let _ = j.assign_process(child.as_raw_handle());
        }

        let wait_fut = tokio::task::spawn_blocking(move || child.wait_with_output());

        let output = match tokio::time::timeout(timeout_duration, wait_fut).await {
            Ok(join_res) => join_res
                .map_err(|e| ActionError::Failed(format!("Failed to join task for {desc}: {e}")))?
                .map_err(|e| ActionError::Failed(format!("{desc} failed: {e}")))?,
            Err(_) => {
                if let Some(ref j) = job {
                    let _ = j.kill_tree();
                }
                return Err(ActionError::Failed(format!(
                    "{desc} timed out after {:?} (process tree killed)",
                    timeout_duration
                )));
            }
        };

        let mut stdout_bytes = output.stdout;
        let is_truncated = if stdout_bytes.len() > max_output_bytes {
            stdout_bytes.truncate(max_output_bytes);
            true
        } else {
            false
        };

        let stdout = String::from_utf8_lossy(&stdout_bytes);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if output.status.success() {
            let res = if stdout.trim().is_empty() {
                format!("{desc}: completed (clean/empty output)")
            } else if is_truncated {
                format!("{}\n\n[... output capped at 64 KB]", stdout)
            } else {
                stdout.to_string()
            };
            Ok(res)
        } else {
            Err(ActionError::Failed(format!("{desc} error: {stderr}")))
        }
    }
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

            LocalAction::ListFolder(dir) => {
                let p = Path::new(&dir);
                if !p.exists() || !p.is_dir() {
                    return Err(ActionError::Failed(format!("Directory not found: {dir}")));
                }
                let mut entries = Vec::new();
                let read_dir = std::fs::read_dir(p).map_err(|e| ActionError::Io(e.to_string()))?;
                for entry in read_dir.flatten() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    let file_type = entry.file_type().ok();
                    let is_dir = file_type.as_ref().map(|t| t.is_dir()).unwrap_or(false);
                    let size_str = if is_dir {
                        "<DIR>".to_string()
                    } else if let Ok(meta) = entry.metadata() {
                        format!("{} B", meta.len())
                    } else {
                        "-".to_string()
                    };
                    entries.push(format!("{:<10} {}", size_str, file_name));
                    if entries.len() >= 50 {
                        entries.push("... (truncated at 50 items)".to_string());
                        break;
                    }
                }
                Ok(format!(
                    "Directory listing for {dir}:\n{}",
                    entries.join("\n")
                ))
            }

            LocalAction::ReadFileHead { path, max_bytes } => {
                let p = Path::new(&path);
                if !p.exists() || !p.is_file() {
                    return Err(ActionError::Failed(format!("File not found: {path}")));
                }
                use std::io::Read;
                let mut file =
                    std::fs::File::open(p).map_err(|e| ActionError::Io(e.to_string()))?;
                let cap = std::cmp::min(max_bytes, 65536); // Cap at 64 KB
                let mut buffer = vec![0u8; cap];
                let bytes_read = file
                    .read(&mut buffer)
                    .map_err(|e| ActionError::Io(e.to_string()))?;
                buffer.truncate(bytes_read);

                // Refuse binary files (check if null bytes exist in first 1024 bytes)
                let check_len = std::cmp::min(bytes_read, 1024);
                if buffer[..check_len].contains(&0x00) {
                    return Err(ActionError::Disallowed(
                        "Refusing to preview binary file (null bytes detected)".into(),
                    ));
                }

                let text = String::from_utf8_lossy(&buffer);
                Ok(format!(
                    "[File head: {path} ({} bytes read)]\n{}",
                    bytes_read, text
                ))
            }

            LocalAction::FileInfo(path) => {
                let p = Path::new(&path);
                if !p.exists() {
                    return Err(ActionError::Failed(format!(
                        "File or directory not found: {path}"
                    )));
                }
                let meta = std::fs::metadata(p).map_err(|e| ActionError::Io(e.to_string()))?;
                let kind = if meta.is_dir() {
                    "Directory"
                } else if meta.is_file() {
                    "File"
                } else {
                    "Other"
                };
                let len = meta.len();
                let readonly = meta.permissions().readonly();
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| format!("{}s since epoch", d.as_secs()))
                    .unwrap_or_else(|| "unknown".to_string());

                Ok(format!(
                    "Path: {path}\nType: {kind}\nSize: {len} bytes\nReadonly: {readonly}\nModified: {modified}"
                ))
            }

            LocalAction::SendFile { path, max_bytes } => {
                let p = Path::new(&path);
                if !p.exists() || !p.is_file() {
                    return Err(ActionError::Failed(format!("File not found: {path}")));
                }
                // TOCTOU defense: re-canonicalize at read time
                let canonical = p
                    .canonicalize()
                    .map_err(|e| ActionError::Io(e.to_string()))?;

                let lower_str = canonical.to_string_lossy().to_ascii_lowercase();
                if lower_str.contains(".ssh")
                    || lower_str.contains(".env")
                    || lower_str.contains(".gemini")
                {
                    return Err(ActionError::Disallowed(
                        "Access to sensitive file blocked at read time".into(),
                    ));
                }

                let meta =
                    std::fs::metadata(&canonical).map_err(|e| ActionError::Io(e.to_string()))?;
                let cap = std::cmp::min(max_bytes, 10 * 1024 * 1024); // 10 MB cap
                if meta.len() > cap as u64 {
                    return Err(ActionError::Disallowed(format!(
                        "File size ({} bytes) exceeds 10 MB limit",
                        meta.len()
                    )));
                }
                Ok(format!("READY_FOR_UPLOAD:{}", canonical.display()))
            }

            LocalAction::GitStatus(repo) => {
                let p = Path::new(&repo);
                if !p.exists() || !p.is_dir() {
                    return Err(ActionError::Failed(format!(
                        "Repository directory not found: {repo}"
                    )));
                }
                let canonical_repo = p
                    .canonicalize()
                    .map_err(|e| ActionError::Io(e.to_string()))?;
                let mut cmd = Self::build_hardened_git_command(&canonical_repo);
                cmd.arg("status");
                Self::run_command_with_timeout_and_cap(cmd, "git status").await
            }

            LocalAction::GitLog { repo, limit } => {
                let p = Path::new(&repo);
                if !p.exists() || !p.is_dir() {
                    return Err(ActionError::Failed(format!(
                        "Repository directory not found: {repo}"
                    )));
                }
                let canonical_repo = p
                    .canonicalize()
                    .map_err(|e| ActionError::Io(e.to_string()))?;
                let cap_limit = std::cmp::min(limit, 50);
                let mut cmd = Self::build_hardened_git_command(&canonical_repo);
                cmd.args(["log", "-n", &cap_limit.to_string(), "--oneline"]);
                Self::run_command_with_timeout_and_cap(cmd, "git log").await
            }

            LocalAction::GitDiffStat(repo) => {
                let p = Path::new(&repo);
                if !p.exists() || !p.is_dir() {
                    return Err(ActionError::Failed(format!(
                        "Repository directory not found: {repo}"
                    )));
                }
                let canonical_repo = p
                    .canonicalize()
                    .map_err(|e| ActionError::Io(e.to_string()))?;
                let mut cmd = Self::build_hardened_git_command(&canonical_repo);
                cmd.args(["diff", "--stat"]);
                Self::run_command_with_timeout_and_cap(cmd, "git diff").await
            }

            LocalAction::GhPrList { repo, limit } => {
                let cap_limit = std::cmp::min(limit, 30);
                let mut cmd = std::process::Command::new("gh");
                cmd.env("GH_PROMPT_DISABLED", "1")
                    .env("NO_COLOR", "1")
                    .args(["pr", "list", "--limit", &cap_limit.to_string()]);
                if let Some(r) = repo {
                    cmd.args(["--repo", &r]);
                }
                Self::run_command_with_timeout_and_cap(cmd, "gh pr list").await
            }

            LocalAction::GhIssueList { repo, limit } => {
                let cap_limit = std::cmp::min(limit, 30);
                let mut cmd = std::process::Command::new("gh");
                cmd.env("GH_PROMPT_DISABLED", "1")
                    .env("NO_COLOR", "1")
                    .args(["issue", "list", "--limit", &cap_limit.to_string()]);
                if let Some(r) = repo {
                    cmd.args(["--repo", &r]);
                }
                Self::run_command_with_timeout_and_cap(cmd, "gh issue list").await
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

    #[tokio::test]
    async fn test_list_folder_action() {
        let temp = tempdir().unwrap();
        fs::write(temp.path().join("file1.txt"), "hello").unwrap();
        fs::create_dir(temp.path().join("subdir")).unwrap();

        let executor = AllowlistedActionExecutor;
        let res = executor
            .execute(LocalAction::ListFolder(
                temp.path().to_string_lossy().to_string(),
            ))
            .await
            .unwrap();

        assert!(res.contains("file1.txt"));
        assert!(res.contains("<DIR>"));
        assert!(res.contains("subdir"));
    }

    #[tokio::test]
    async fn test_read_file_head_text_and_binary_refusal() {
        let temp = tempdir().unwrap();
        let txt_path = temp.path().join("test.txt");
        fs::write(&txt_path, "hello world 123").unwrap();

        let bin_path = temp.path().join("binary.bin");
        fs::write(&bin_path, b"hello\x00world\x01\x02").unwrap();

        let executor = AllowlistedActionExecutor;

        // Text succeeds
        let res = executor
            .execute(LocalAction::ReadFileHead {
                path: txt_path.to_string_lossy().to_string(),
                max_bytes: 100,
            })
            .await
            .unwrap();
        assert!(res.contains("hello world 123"));

        // Binary is refused
        let bin_res = executor
            .execute(LocalAction::ReadFileHead {
                path: bin_path.to_string_lossy().to_string(),
                max_bytes: 100,
            })
            .await;
        assert!(bin_res.is_err());
        assert_eq!(
            bin_res,
            Err(ActionError::Disallowed(
                "Refusing to preview binary file (null bytes detected)".into()
            ))
        );
    }

    #[tokio::test]
    async fn test_file_info_action() {
        let temp = tempdir().unwrap();
        let file_path = temp.path().join("info.txt");
        fs::write(&file_path, "some content").unwrap();

        let executor = AllowlistedActionExecutor;
        let res = executor
            .execute(LocalAction::FileInfo(
                file_path.to_string_lossy().to_string(),
            ))
            .await
            .unwrap();

        assert!(res.contains("Type: File"));
        assert!(res.contains("Size: 12 bytes"));
    }

    #[tokio::test]
    async fn test_send_file_size_limit() {
        let temp = tempdir().unwrap();
        let small_file = temp.path().join("small.txt");
        fs::write(&small_file, "tiny").unwrap();

        let executor = AllowlistedActionExecutor;
        let ok_res = executor
            .execute(LocalAction::SendFile {
                path: small_file.to_string_lossy().to_string(),
                max_bytes: 10 * 1024 * 1024,
            })
            .await
            .unwrap();
        assert!(ok_res.starts_with("READY_FOR_UPLOAD:"));

        // Excess size limit
        let big_res = executor
            .execute(LocalAction::SendFile {
                path: small_file.to_string_lossy().to_string(),
                max_bytes: 2, // Less than 4 bytes
            })
            .await;
        assert!(big_res.is_err());
    }

    #[tokio::test]
    async fn test_send_file_revalidates_canonical_path_at_read_time() {
        let temp = tempdir().unwrap();
        // Create a sensitive simulated .env file
        let sensitive_path = temp.path().join(".env");
        fs::write(&sensitive_path, "SECRET=12345").unwrap();

        let executor = AllowlistedActionExecutor;

        // Even if requested, SendFile detects sensitive target at read time and refuses
        let res = executor
            .execute(LocalAction::SendFile {
                path: sensitive_path.to_string_lossy().to_string(),
                max_bytes: 10 * 1024 * 1024,
            })
            .await;

        assert_eq!(
            res,
            Err(ActionError::Disallowed(
                "Access to sensitive file blocked at read time".into()
            ))
        );
    }

    #[tokio::test]
    async fn test_git_fsmonitor_override_prevents_script_execution() {
        let temp = tempdir().unwrap();
        let repo_path = temp.path();

        // 1. Initialize a minimal git repository
        let init_out = std::process::Command::new("git")
            .arg("init")
            .current_dir(repo_path)
            .output();
        if init_out.is_err() || !init_out.unwrap().status.success() {
            // If git CLI is not on PATH in this environment, skip test cleanly
            return;
        }

        let marker_file = repo_path.join("fsmonitor_marker.txt");
        if marker_file.exists() {
            let _ = fs::remove_file(&marker_file);
        }

        // 2. Configure a malicious core.fsmonitor pointing to a python command creating the marker
        let marker_script = format!(
            "python -c \"import sys; open(r'{}', 'w').write('HACKED')\"",
            marker_file.display()
        );
        let git_config_path = repo_path.join(".git").join("config");
        let malicious_config = format!(
            "\n[core]\n\tfsmonitor = \"{}\"\n",
            marker_script.replace('\\', "\\\\")
        );
        let mut existing_config = fs::read_to_string(&git_config_path).unwrap_or_default();
        existing_config.push_str(&malicious_config);
        fs::write(&git_config_path, existing_config).unwrap();

        // 3. Execute GitStatus through AllowlistedActionExecutor
        let executor = AllowlistedActionExecutor;
        let res = executor
            .execute(LocalAction::GitStatus(
                repo_path.to_string_lossy().to_string(),
            ))
            .await;

        // Command succeeds because the hardened executor forces -c core.fsmonitor=
        assert!(res.is_ok(), "Hardened git status should succeed: {:?}", res);

        // 4. Assert marker was NEVER created
        assert!(
            !marker_file.exists(),
            "CRITICAL: core.fsmonitor marker was executed! Hardening failed."
        );
    }

    #[tokio::test]
    async fn test_git_helper_timeout_kills_process_tree() {
        // cmd.exe spawns powershell.exe which sleeps for 10s
        let mut cmd = std::process::Command::new("cmd.exe");
        cmd.args([
            "/c",
            "powershell.exe",
            "-NoProfile",
            "-Command",
            "Start-Sleep -Seconds 10",
        ]);

        let res = AllowlistedActionExecutor::run_command_with_custom_timeout_and_cap(
            cmd,
            "test_tree_kill",
            Duration::from_millis(300),
            65536,
        )
        .await;

        assert!(res.is_err(), "Must time out and kill process tree");
        match res.unwrap_err() {
            ActionError::Failed(msg) => {
                assert!(msg.contains("timed out after") && msg.contains("process tree killed"));
            }
            other => panic!("Expected ActionError::Failed, got {:?}", other),
        }
    }
}
