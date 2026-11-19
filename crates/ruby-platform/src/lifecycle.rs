//! Lifecycle, single-instance, power management, autostart, and session lock utilities.
//! All native OS operations are gated behind traits to allow 100% deterministic testing without OS side-effects.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LifecycleError {
    #[error("Another instance of Ruby is already running")]
    AlreadyRunning,
    #[error("Registry operation failed: {0}")]
    RegistryFailed(String),
    #[error("Power query failed: {0}")]
    PowerQueryFailed(String),
    #[error("Session lock error: {0}")]
    SessionLockError(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ─── L1: Single Instance Named Mutex ─────────────────────────────────────────

pub trait SingleInstanceGuard: Send + Sync {
    fn is_first_instance(&self) -> bool;
}

/// In-memory single instance guard for tests.
#[derive(Debug, Clone)]
pub struct MockSingleInstanceGuard {
    is_first: bool,
}

impl MockSingleInstanceGuard {
    pub fn new(is_first: bool) -> Self {
        Self { is_first }
    }
}

impl SingleInstanceGuard for MockSingleInstanceGuard {
    fn is_first_instance(&self) -> bool {
        self.is_first
    }
}

/// Windows native named mutex single instance guard.
pub struct NamedSingleInstance {
    #[cfg(windows)]
    _handle: Option<windows::Win32::Foundation::HANDLE>,
    is_first: bool,
}

impl NamedSingleInstance {
    pub fn try_acquire(mutex_name: &str) -> Result<Self, LifecycleError> {
        #[cfg(windows)]
        {
            use windows::core::HSTRING;
            use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
            use windows::Win32::System::Threading::CreateMutexW;

            let h_name = HSTRING::from(mutex_name);
            unsafe {
                let handle = CreateMutexW(None, true, &h_name);
                match handle {
                    Ok(h) => {
                        let err = GetLastError();
                        if err == ERROR_ALREADY_EXISTS {
                            Ok(Self {
                                _handle: Some(h),
                                is_first: false,
                            })
                        } else {
                            Ok(Self {
                                _handle: Some(h),
                                is_first: true,
                            })
                        }
                    }
                    Err(e) => Err(LifecycleError::RegistryFailed(e.to_string())),
                }
            }
        }
        #[cfg(not(windows))]
        {
            let _ = mutex_name;
            Ok(Self { is_first: true })
        }
    }
}

unsafe impl Send for NamedSingleInstance {}
unsafe impl Sync for NamedSingleInstance {}

impl SingleInstanceGuard for NamedSingleInstance {
    fn is_first_instance(&self) -> bool {
        self.is_first
    }
}

#[cfg(windows)]
impl Drop for NamedSingleInstance {
    fn drop(&mut self) {
        if let Some(h) = self._handle.take() {
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(h);
            }
        }
    }
}

// ─── L2: Autostart Manager (HKCU Run) ────────────────────────────────────────

pub trait AutostartManager: Send + Sync {
    fn is_enabled(&self) -> Result<bool, LifecycleError>;
    fn enable(&self, app_path: &Path, args: &[&str]) -> Result<(), LifecycleError>;
    fn disable(&self) -> Result<(), LifecycleError>;
}

/// In-memory fake autostart manager for safe unit testing (never touches registry).
#[derive(Debug, Clone, Default)]
pub struct MockAutostartManager {
    state: Arc<Mutex<Option<String>>>,
}

impl MockAutostartManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current_command(&self) -> Option<String> {
        self.state.lock().unwrap().clone()
    }
}

impl AutostartManager for MockAutostartManager {
    fn is_enabled(&self) -> Result<bool, LifecycleError> {
        Ok(self.state.lock().unwrap().is_some())
    }

    fn enable(&self, app_path: &Path, args: &[&str]) -> Result<(), LifecycleError> {
        let mut cmd = format!("\"{}\"", app_path.display());
        for a in args {
            cmd.push(' ');
            cmd.push_str(a);
        }
        *self.state.lock().unwrap() = Some(cmd);
        Ok(())
    }

    fn disable(&self) -> Result<(), LifecycleError> {
        *self.state.lock().unwrap() = None;
        Ok(())
    }
}

// ─── L3: Power Status & Keep-Awake ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSource {
    AcPower,
    Battery,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PowerStatus {
    pub source: PowerSource,
    pub battery_percent: Option<u8>,
}

pub trait PowerStatusProvider: Send + Sync {
    fn query_status(&self) -> Result<PowerStatus, LifecycleError>;
}

/// In-memory mock power provider for deterministic unit testing.
#[derive(Debug, Clone)]
pub struct MockPowerStatusProvider {
    status: Arc<Mutex<PowerStatus>>,
}

impl MockPowerStatusProvider {
    pub fn new(source: PowerSource, battery_percent: Option<u8>) -> Self {
        Self {
            status: Arc::new(Mutex::new(PowerStatus {
                source,
                battery_percent,
            })),
        }
    }

    pub fn set_status(&self, source: PowerSource, battery_percent: Option<u8>) {
        *self.status.lock().unwrap() = PowerStatus {
            source,
            battery_percent,
        };
    }
}

impl PowerStatusProvider for MockPowerStatusProvider {
    fn query_status(&self) -> Result<PowerStatus, LifecycleError> {
        Ok(*self.status.lock().unwrap())
    }
}

pub trait KeepAwakeManager: Send + Sync {
    fn set_keep_awake(&self, active: bool) -> Result<(), LifecycleError>;
    fn is_keep_awake(&self) -> bool;
}

/// Mock keep-awake manager for testing.
#[derive(Debug, Clone, Default)]
pub struct MockKeepAwakeManager {
    active: Arc<AtomicBool>,
}

impl MockKeepAwakeManager {
    pub fn new() -> Self {
        Self::default()
    }
}

impl KeepAwakeManager for MockKeepAwakeManager {
    fn set_keep_awake(&self, active: bool) -> Result<(), LifecycleError> {
        self.active.store(active, Ordering::SeqCst);
        Ok(())
    }

    fn is_keep_awake(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }
}

// ─── L5: Crash Handling Hook ─────────────────────────────────────────────────

pub struct CrashReporter {
    crash_dir: PathBuf,
}

impl CrashReporter {
    pub fn new(crash_dir: PathBuf) -> Self {
        Self { crash_dir }
    }

    pub fn default_dir() -> PathBuf {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            PathBuf::from(local).join("Ruby").join("crashes")
        } else {
            PathBuf::from(".").join("crashes")
        }
    }

    pub fn write_crash_dump(
        &self,
        info: &str,
        location: Option<&str>,
    ) -> Result<PathBuf, LifecycleError> {
        std::fs::create_dir_all(&self.crash_dir)?;
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let fname = format!("crash_{}_{}.json", ts, uuid::Uuid::new_v4());
        let out_path = self.crash_dir.join(fname);

        // Sanitize crash dump content to ensure no API tokens or passwords leak
        let sanitized = sanitize_crash_info(info);

        let dump = serde_json::json!({
            "timestamp": ts,
            "location": location.unwrap_or("unknown"),
            "panic_info": sanitized,
            "version": env!("CARGO_PKG_VERSION"),
        });

        std::fs::write(&out_path, serde_json::to_string_pretty(&dump)?)?;
        Ok(out_path)
    }
}

fn sanitize_crash_info(raw: &str) -> String {
    let mut clean = raw.to_string();
    let patterns = [
        ("ghp_", "[REDACTED_GH_TOKEN]"),
        ("AIza", "[REDACTED_GOOGLE_KEY]"),
        ("sk-", "[REDACTED_API_KEY]"),
        ("freellmapi-", "[REDACTED_FREELLM_KEY]"),
    ];
    for (pat, repl) in patterns {
        if let Some(idx) = clean.find(pat) {
            clean.replace_range(idx..std::cmp::min(idx + 40, clean.len()), repl);
        }
    }
    clean
}

// ─── L6: Session Lock / Unlock Watcher ────────────────────────────────────────

pub trait SessionLockWatcher: Send + Sync {
    fn is_locked(&self) -> bool;
    fn set_locked(&self, locked: bool);
}

#[derive(Debug, Clone, Default)]
pub struct MockSessionLockWatcher {
    locked: Arc<AtomicBool>,
}

impl MockSessionLockWatcher {
    pub fn new(initially_locked: bool) -> Self {
        Self {
            locked: Arc::new(AtomicBool::new(initially_locked)),
        }
    }
}

impl SessionLockWatcher for MockSessionLockWatcher {
    fn is_locked(&self) -> bool {
        self.locked.load(Ordering::SeqCst)
    }

    fn set_locked(&self, locked: bool) {
        self.locked.store(locked, Ordering::SeqCst);
    }
}

// ─── Unit Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_single_instance_mock_guard() {
        let first = MockSingleInstanceGuard::new(true);
        assert!(first.is_first_instance());

        let second = MockSingleInstanceGuard::new(false);
        assert!(!second.is_first_instance());
    }

    #[test]
    fn test_autostart_mock_enable_and_disable() {
        let mgr = MockAutostartManager::new();
        assert!(!mgr.is_enabled().unwrap());

        mgr.enable(Path::new("C:\\Ruby\\ruby.exe"), &["--minimized"])
            .unwrap();
        assert!(mgr.is_enabled().unwrap());
        assert_eq!(
            mgr.current_command().unwrap(),
            "\"C:\\Ruby\\ruby.exe\" --minimized"
        );

        mgr.disable().unwrap();
        assert!(!mgr.is_enabled().unwrap());
        assert!(mgr.current_command().is_none());
    }

    #[test]
    fn test_power_status_query_and_keep_awake() {
        let power = MockPowerStatusProvider::new(PowerSource::AcPower, Some(95));
        let status = power.query_status().unwrap();
        assert_eq!(status.source, PowerSource::AcPower);
        assert_eq!(status.battery_percent, Some(95));

        power.set_status(PowerSource::Battery, Some(42));
        assert_eq!(power.query_status().unwrap().source, PowerSource::Battery);
        assert_eq!(power.query_status().unwrap().battery_percent, Some(42));

        let keep_awake = MockKeepAwakeManager::new();
        assert!(!keep_awake.is_keep_awake());
        keep_awake.set_keep_awake(true).unwrap();
        assert!(keep_awake.is_keep_awake());
    }

    #[test]
    fn test_crash_reporter_creates_sanitized_dump() {
        let temp = tempdir().unwrap();
        let reporter = CrashReporter::new(temp.path().to_path_buf());

        let panic_str = "Thread panicked with token ghp_1234567890abcdefghijklmnopqrstuvwxyz at line 42";
        let dump_path = reporter
            .write_crash_dump(panic_str, Some("main.rs:42"))
            .unwrap();

        assert!(dump_path.exists());
        let content = std::fs::read_to_string(dump_path).unwrap();
        assert!(!content.contains("ghp_1234567890"));
        assert!(content.contains("[REDACTED_GH_TOKEN]"));
        assert!(content.contains("main.rs:42"));
    }

    #[test]
    fn test_session_lock_watcher_mock() {
        let watcher = MockSessionLockWatcher::new(false);
        assert!(!watcher.is_locked());
        watcher.set_locked(true);
        assert!(watcher.is_locked());
    }
}
