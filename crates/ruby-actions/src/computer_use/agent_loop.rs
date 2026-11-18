//! Mode A Live Computer Use Autonomous Agent Loop with safety boundaries,
//! kill switch integration, stuck detection, and hermetic mock execution.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

use super::action::ComputerAction;
use super::executor::{ComputerUseExecutor, ScreenCapture};
use ruby_policy::computer_policy::{
    check_secret_typing, detect_consequential_intent, is_window_denylisted_with_custom,
};
use ruby_policy::ConfirmationBroker;

/// Helper to detect if active window belongs to a web browser.
pub fn is_browser_target(proc_name: &str, win_title: &str) -> bool {
    let lower_proc = proc_name.to_ascii_lowercase();
    let lower_title = win_title.to_ascii_lowercase();
    let browser_keywords = [
        "chrome", "msedge", "firefox", "brave", "opera", "vivaldi", "arc",
    ];
    for b in browser_keywords {
        if lower_proc.contains(b) || lower_title.contains(b) {
            return true;
        }
    }
    false
}

/// Abstract security and elevation inspector for the active desktop session.
#[async_trait]
pub trait PlatformSecurityInspector: Send + Sync {
    async fn is_session_locked(&self) -> bool;
    async fn is_foreground_elevated_or_secure_desktop(&self) -> bool;
    async fn check_type_guardrails(&self, is_browser: bool) -> Result<(), String>;
}

/// Mock Platform Security Inspector for offline and hermetic testing.
#[derive(Default, Clone)]
pub struct MockPlatformSecurityInspector {
    pub session_locked: bool,
    pub foreground_elevated: bool,
    pub password_field_detected: bool,
    pub unverifiable_browser_field: bool,
}

#[async_trait]
impl PlatformSecurityInspector for MockPlatformSecurityInspector {
    async fn is_session_locked(&self) -> bool {
        self.session_locked
    }

    async fn is_foreground_elevated_or_secure_desktop(&self) -> bool {
        self.foreground_elevated
    }

    async fn check_type_guardrails(&self, is_browser: bool) -> Result<(), String> {
        if self.password_field_detected {
            return Err("Focused control is a password field; typing aborted".into());
        }
        if is_browser && self.unverifiable_browser_field {
            return Err("Unverifiable browser input field: failing closed".into());
        }
        Ok(())
    }
}

/// Native Windows implementation of PlatformSecurityInspector.
#[cfg(windows)]
pub struct WindowsPlatformSecurityInspector;

#[cfg(windows)]
#[async_trait]
impl PlatformSecurityInspector for WindowsPlatformSecurityInspector {
    async fn is_session_locked(&self) -> bool {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        unsafe {
            let hwnd = GetForegroundWindow();
            hwnd.0.is_null()
        }
    }

    async fn is_foreground_elevated_or_secure_desktop(&self) -> bool {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return false;
            }
            ruby_platform::elevation_check::is_target_window_elevated(hwnd).unwrap_or(false)
                && !ruby_platform::elevation_check::is_current_process_elevated()
        }
    }

    async fn check_type_guardrails(&self, is_browser: bool) -> Result<(), String> {
        if is_browser {
            match ruby_platform::elevation_check::is_focused_element_password_uia() {
                Ok(true) => Err("Focused control is a password field; typing aborted".into()),
                Ok(false) => Ok(()),
                Err(e) => Err(format!(
                    "Unverifiable browser input field; failing closed: {e}"
                )),
            }
        } else {
            ruby_platform::elevation_check::check_password_guardrails()
                .map_err(|e| format!("Password guardrail triggered: {e}"))
        }
    }
}

/// Async and atomic kill switch cancellation token.
#[derive(Clone, Default)]
pub struct KillSwitchToken {
    cancelled: Arc<AtomicBool>,
    notify: Arc<tokio::sync::Notify>,
}

impl KillSwitchToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRecord {
    pub step: usize,
    pub timestamp: DateTime<Utc>,
    pub action_type: String,
    pub coordinates: Option<(u32, u32)>,
    pub target_window_title: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Completed(Option<String>),
    Failed(String),
    Cancelled,
}

#[async_trait]
pub trait ComputerUseModelClient: Send + Sync {
    async fn propose_action(
        &self,
        goal: &str,
        step: usize,
        step_history: &[StepRecord],
        screen: &ScreenCapture,
    ) -> Result<ComputerAction, String>;
}

pub struct ComputerUseAgentConfig {
    pub max_steps: usize,
    pub wall_clock_timeout: Duration,
    pub per_action_timeout: Duration,
    pub dry_run: bool,
    pub denylist: Vec<String>,
}

impl Default for ComputerUseAgentConfig {
    fn default() -> Self {
        Self {
            max_steps: 30,
            wall_clock_timeout: Duration::from_secs(300),
            per_action_timeout: Duration::from_secs(10),
            dry_run: false,
            denylist: Vec::new(),
        }
    }
}

pub struct ComputerUseAgentLoop {
    pub config: ComputerUseAgentConfig,
    pub executor: Arc<dyn ComputerUseExecutor>,
    pub model_client: Arc<dyn ComputerUseModelClient>,
    pub confirmation_broker: Option<Arc<dyn ConfirmationBroker>>,
    pub cancellation_token: KillSwitchToken,
    pub security_inspector: Arc<dyn PlatformSecurityInspector>,
    pub audit_log: Arc<std::sync::Mutex<Vec<StepRecord>>>,
    pub window_override: Arc<std::sync::Mutex<Option<(String, String)>>>,
}

impl ComputerUseAgentLoop {
    pub fn new(
        config: ComputerUseAgentConfig,
        executor: Arc<dyn ComputerUseExecutor>,
        model_client: Arc<dyn ComputerUseModelClient>,
        confirmation_broker: Option<Arc<dyn ConfirmationBroker>>,
        cancellation_token: KillSwitchToken,
    ) -> Self {
        Self {
            config,
            executor,
            model_client,
            confirmation_broker,
            cancellation_token,
            security_inspector: Arc::new(MockPlatformSecurityInspector::default()),
            audit_log: Arc::new(std::sync::Mutex::new(Vec::new())),
            window_override: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    #[cfg(windows)]
    pub fn with_windows_inspector(mut self) -> Self {
        self.security_inspector = Arc::new(WindowsPlatformSecurityInspector);
        self
    }

    pub fn with_security_inspector(
        mut self,
        inspector: Arc<dyn PlatformSecurityInspector>,
    ) -> Self {
        self.security_inspector = inspector;
        self
    }

    pub fn with_window_override(
        self,
        title: impl Into<String>,
        proc_name: impl Into<String>,
    ) -> Self {
        *self.window_override.lock().unwrap() = Some((title.into(), proc_name.into()));
        self
    }

    pub fn get_audit_log(&self) -> Vec<StepRecord> {
        self.audit_log.lock().unwrap().clone()
    }

    /// Helper to get current foreground window title and process name safely.
    fn inspect_active_window(&self) -> (String, String) {
        if let Some(pair) = self.window_override.lock().unwrap().clone() {
            return pair;
        }
        get_active_window_info()
    }
}

/// Helper to get current foreground window title and process name safely on Windows or mock environments.
pub fn get_active_window_info() -> (String, String) {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};
        unsafe {
            let hwnd: HWND = GetForegroundWindow();
            if hwnd.0.is_null() {
                return ("Unknown".into(), "unknown.exe".into());
            }
            let mut title_buf = [0u16; 512];
            let len = GetWindowTextW(hwnd, &mut title_buf);
            let title = String::from_utf16_lossy(&title_buf[..len as usize]);
            let proc_name = ruby_platform::elevation_check::get_foreground_process_name()
                .unwrap_or_else(|| "active_process.exe".into());
            (title, proc_name)
        }
    }
    #[cfg(not(windows))]
    {
        ("Test Desktop".into(), "test.exe".into())
    }
}

impl ComputerUseAgentLoop {
    /// Executes the autonomous Mode A loop until completion, failure, or cancellation.
    pub async fn run(&self, goal: &str) -> TaskStatus {
        info!("Starting Mode A Computer Use Loop for goal: \"{}\"", goal);
        let start_time = Instant::now();
        let mut step_history: Vec<StepRecord> = Vec::new();
        let mut recent_actions: Vec<ComputerAction> = Vec::new();

        for step in 1..=self.config.max_steps {
            // 1. Check Global Kill Switch
            if self.cancellation_token.is_cancelled() {
                warn!("Kill switch activated! Aborting Computer Use task immediately.");
                let _ = self.executor.release_all_held_inputs().await;
                return TaskStatus::Cancelled;
            }

            // 2. Check Wall-Clock Timeout
            if start_time.elapsed() >= self.config.wall_clock_timeout {
                warn!(
                    "Task exceeded wall-clock timeout of {:?}",
                    self.config.wall_clock_timeout
                );
                let _ = self.executor.release_all_held_inputs().await;
                return TaskStatus::Failed("Wall-clock timeout exceeded".into());
            }

            // 3. Platform Security Checks: Session Lock & Elevation / Secure Desktop
            if self.security_inspector.is_session_locked().await {
                warn!("Windows user session is locked. Halting Computer Use.");
                let _ = self.executor.release_all_held_inputs().await;
                return TaskStatus::Failed("Session is locked".into());
            }

            if self
                .security_inspector
                .is_foreground_elevated_or_secure_desktop()
                .await
            {
                warn!("Foreground window is elevated or secure desktop/UAC is active. Halting.");
                let _ = self.executor.release_all_held_inputs().await;
                return TaskStatus::Failed(
                    "Target window is elevated or secure desktop active".into(),
                );
            }

            // 4. Inspect Active Window & Denylist Gate
            let (win_title, proc_name) = self.inspect_active_window();
            if is_window_denylisted_with_custom(&win_title, &proc_name, &self.config.denylist) {
                warn!(
                    "Active window \"{}\" ({}) is in the Security Denylist! Halting.",
                    win_title, proc_name
                );
                let _ = self.executor.release_all_held_inputs().await;
                return TaskStatus::Failed(format!(
                    "Denied: Target window \"{}\" is on the security denylist",
                    win_title
                ));
            }

            // 4. Capture Desktop Screenshot
            let screen_capture = match self.executor.capture_screen().await {
                Ok(sc) => sc,
                Err(e) => {
                    warn!("Screenshot capture failed: {:?}", e);
                    let _ = self.executor.release_all_held_inputs().await;
                    return TaskStatus::Failed(format!("Screen capture failed: {e}"));
                }
            };

            // 5. Query Model Client (with select! on Kill Switch so hangs abort instantly)
            let proposed_action = tokio::select! {
                _ = self.cancellation_token.cancelled() => {
                    warn!("Kill switch triggered while waiting for model reasoning!");
                    let _ = self.executor.release_all_held_inputs().await;
                    return TaskStatus::Cancelled;
                }
                res = self.model_client.propose_action(goal, step, &step_history, &screen_capture) => {
                    match res {
                        Ok(act) => act,
                        Err(e) => {
                            warn!("Model reasoning failed on step {}: {}", step, e);
                            let _ = self.executor.release_all_held_inputs().await;
                            return TaskStatus::Failed(format!("Model call failed: {e}"));
                        }
                    }
                }
            };

            // 6. Loop Limit: Stuck Detector (repeated identical action 3x)
            if recent_actions.len() >= 2 && recent_actions.iter().all(|a| a == &proposed_action) {
                warn!(
                    "Stuck Detector triggered: identical action {:?} repeated 3 times. Stopping.",
                    proposed_action
                );
                let _ = self.executor.release_all_held_inputs().await;
                return TaskStatus::Failed("Stuck detector: same action repeated 3 times".into());
            }

            // 7. Security Policy Checks
            match &proposed_action {
                ComputerAction::TypeText { text, .. } => {
                    // Refuse secret typing
                    if let Err(e) = check_secret_typing(text) {
                        warn!("Secret typing blocked by policy: {:?}", e);
                        let _ = self.executor.release_all_held_inputs().await;
                        return TaskStatus::Failed("Policy blocked secret typing".into());
                    }

                    // Password guardrail & browser verification check
                    let is_browser = is_browser_target(&proc_name, &win_title);
                    if let Err(e) = self
                        .security_inspector
                        .check_type_guardrails(is_browser)
                        .await
                    {
                        warn!("Type guardrail prevented typing: {}", e);
                        let _ = self.executor.release_all_held_inputs().await;
                        return TaskStatus::Failed(format!(
                            "Security policy prevented typing: {e}"
                        ));
                    }
                }
                ComputerAction::Finish { result } => {
                    info!("Goal achieved on step {}: {:?}", step, result);
                    let _ = self.executor.release_all_held_inputs().await;
                    return TaskStatus::Completed(result.clone());
                }
                ComputerAction::Fail { reason } => {
                    warn!("Agent reported failure on step {}: {}", step, reason);
                    let _ = self.executor.release_all_held_inputs().await;
                    return TaskStatus::Failed(reason.clone());
                }
                _ => {}
            }

            // 8. Consequential Action Confirmation Gating
            let action_desc = format!("{:?} on window \"{}\"", proposed_action, win_title);
            if let Some(kind) = detect_consequential_intent(&action_desc) {
                info!(
                    "Consequential action detected ({:?}) - requesting confirmation",
                    kind
                );
                if let Some(broker) = &self.confirmation_broker {
                    match broker.request_confirmation(&action_desc).await {
                        Ok(true) => {
                            info!("User authorized consequential action.");
                        }
                        _ => {
                            warn!(
                                "User rejected or confirmation timed out for consequential action."
                            );
                            let _ = self.executor.release_all_held_inputs().await;
                            return TaskStatus::Failed(
                                "Consequential action not authorized".into(),
                            );
                        }
                    }
                }
            }

            // 9. Execute Action via Executor
            let exec_status = if self.config.dry_run {
                "DryRun".to_string()
            } else {
                match self.executor.execute_action(&proposed_action).await {
                    Ok(_) => "Executed".to_string(),
                    Err(e) => {
                        warn!("Action execution error on step {}: {:?}", step, e);
                        format!("Error: {e}")
                    }
                }
            };

            // 10. Audit Logging (Zero secrets, zero raw screenshots on disk)
            let record = StepRecord {
                step,
                timestamp: Utc::now(),
                action_type: proposed_action.action_type_str().to_string(),
                coordinates: proposed_action.coordinates(),
                target_window_title: win_title,
                status: exec_status,
            };

            step_history.push(record.clone());
            self.audit_log.lock().unwrap().push(record);

            if recent_actions.len() >= 2 {
                recent_actions.remove(0);
            }
            recent_actions.push(proposed_action);

            // Per-action cooperative yield
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        let _ = self.executor.release_all_held_inputs().await;
        TaskStatus::Failed(format!(
            "Exceeded maximum step limit ({})",
            self.config.max_steps
        ))
    }
}

/// Mock Model Client for testing agent loop behavior, stuck detection, and aborts.
pub struct MockComputerUseModelClient {
    pub actions_queue: std::sync::Mutex<Vec<ComputerAction>>,
    pub hang_delay: Option<Duration>,
}

impl MockComputerUseModelClient {
    pub fn new(actions: Vec<ComputerAction>) -> Self {
        Self {
            actions_queue: std::sync::Mutex::new(actions),
            hang_delay: None,
        }
    }

    pub fn with_hang(hang_delay: Duration) -> Self {
        Self {
            actions_queue: std::sync::Mutex::new(Vec::new()),
            hang_delay: Some(hang_delay),
        }
    }
}

#[async_trait]
impl ComputerUseModelClient for MockComputerUseModelClient {
    async fn propose_action(
        &self,
        _goal: &str,
        _step: usize,
        _step_history: &[StepRecord],
        _screen: &ScreenCapture,
    ) -> Result<ComputerAction, String> {
        if let Some(delay) = self.hang_delay {
            tokio::time::sleep(delay).await;
        }

        let mut queue = self.actions_queue.lock().unwrap();
        if queue.is_empty() {
            Ok(ComputerAction::Finish {
                result: Some("Completed naturally".into()),
            })
        } else {
            Ok(queue.remove(0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computer_use::action::MouseButton;
    use crate::computer_use::executor::MockComputerUseExecutor;

    #[tokio::test]
    async fn test_agent_loop_success_path() {
        let executor = Arc::new(MockComputerUseExecutor::default());
        let model = Arc::new(MockComputerUseModelClient::new(vec![
            ComputerAction::Click {
                x: 100,
                y: 100,
                button: MouseButton::Left,
            },
            ComputerAction::TypeText {
                text: "hello world".into(),
                press_enter: true,
            },
            ComputerAction::Finish {
                result: Some("Done typing".into()),
            },
        ]));

        let cancel_token = KillSwitchToken::new();
        let agent = ComputerUseAgentLoop::new(
            ComputerUseAgentConfig::default(),
            Arc::clone(&executor) as Arc<dyn ComputerUseExecutor>,
            Arc::clone(&model) as Arc<dyn ComputerUseModelClient>,
            None,
            cancel_token,
        );

        let res = agent.run("Type hello world").await;
        assert_eq!(res, TaskStatus::Completed(Some("Done typing".into())));

        let audit = agent.get_audit_log();
        assert_eq!(audit.len(), 2); // 2 steps executed before Finish
        assert_eq!(audit[0].action_type, "click");
        assert_eq!(audit[1].action_type, "type_text");
    }

    #[tokio::test]
    async fn test_kill_switch_aborts_hanging_model_call() {
        let executor = Arc::new(MockComputerUseExecutor::default());
        // Model configured to hang for 10 seconds
        let model = Arc::new(MockComputerUseModelClient::with_hang(Duration::from_secs(
            10,
        )));
        let cancel_token = KillSwitchToken::new();

        let cancel_token_clone = cancel_token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            cancel_token_clone.cancel();
        });

        let agent = ComputerUseAgentLoop::new(
            ComputerUseAgentConfig::default(),
            Arc::clone(&executor) as Arc<dyn ComputerUseExecutor>,
            Arc::clone(&model) as Arc<dyn ComputerUseModelClient>,
            None,
            cancel_token,
        );

        let start = Instant::now();
        let res = agent.run("Hanging goal").await;
        assert_eq!(res, TaskStatus::Cancelled);
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn test_stuck_detector_aborts_loop() {
        let executor = Arc::new(MockComputerUseExecutor::default());
        let repeated_action = ComputerAction::Click {
            x: 200,
            y: 200,
            button: MouseButton::Left,
        };
        // 4 identical actions
        let model = Arc::new(MockComputerUseModelClient::new(vec![
            repeated_action.clone(),
            repeated_action.clone(),
            repeated_action.clone(),
            repeated_action.clone(),
        ]));

        let cancel_token = KillSwitchToken::new();
        let agent = ComputerUseAgentLoop::new(
            ComputerUseAgentConfig::default(),
            Arc::clone(&executor) as Arc<dyn ComputerUseExecutor>,
            Arc::clone(&model) as Arc<dyn ComputerUseModelClient>,
            None,
            cancel_token,
        );

        let res = agent.run("Looping goal").await;
        assert!(matches!(res, TaskStatus::Failed(msg) if msg.contains("Stuck detector")));
    }

    #[tokio::test]
    async fn test_secret_typing_refused() {
        let executor = Arc::new(MockComputerUseExecutor::default());
        let dummy_key = ["AIza", "SyD-76543210_abcdefghijklmnopqrstuv"].concat();
        let model = Arc::new(MockComputerUseModelClient::new(vec![
            ComputerAction::TypeText {
                text: format!("My secret is {dummy_key}"),
                press_enter: true,
            },
        ]));

        let cancel_token = KillSwitchToken::new();
        let agent = ComputerUseAgentLoop::new(
            ComputerUseAgentConfig::default(),
            Arc::clone(&executor) as Arc<dyn ComputerUseExecutor>,
            Arc::clone(&model) as Arc<dyn ComputerUseModelClient>,
            None,
            cancel_token,
        );

        let res = agent.run("Type key").await;
        assert!(matches!(res, TaskStatus::Failed(msg) if msg.contains("secret typing")));
    }

    #[tokio::test]
    async fn test_dry_run_mode_records_without_executing() {
        let executor = Arc::new(MockComputerUseExecutor::default());
        let model = Arc::new(MockComputerUseModelClient::new(vec![
            ComputerAction::Click {
                x: 300,
                y: 300,
                button: MouseButton::Left,
            },
            ComputerAction::Finish { result: None },
        ]));

        let config = ComputerUseAgentConfig {
            dry_run: true,
            ..Default::default()
        };

        let cancel_token = KillSwitchToken::new();
        let agent = ComputerUseAgentLoop::new(
            config,
            Arc::clone(&executor) as Arc<dyn ComputerUseExecutor>,
            Arc::clone(&model) as Arc<dyn ComputerUseModelClient>,
            None,
            cancel_token,
        );

        let res = agent.run("Dry run test").await;
        assert_eq!(res, TaskStatus::Completed(None));

        let audit = agent.get_audit_log();
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].status, "DryRun");
    }

    #[tokio::test]
    async fn test_max_steps_limit_exceeded() {
        let executor = Arc::new(MockComputerUseExecutor::default());
        let actions = vec![
            ComputerAction::Move { x: 10, y: 10 },
            ComputerAction::Move { x: 20, y: 20 },
            ComputerAction::Move { x: 30, y: 30 },
            ComputerAction::Move { x: 40, y: 40 },
        ];
        let model = Arc::new(MockComputerUseModelClient::new(actions));

        let config = ComputerUseAgentConfig {
            max_steps: 3,
            ..Default::default()
        };

        let cancel_token = KillSwitchToken::new();
        let agent = ComputerUseAgentLoop::new(
            config,
            Arc::clone(&executor) as Arc<dyn ComputerUseExecutor>,
            Arc::clone(&model) as Arc<dyn ComputerUseModelClient>,
            None,
            cancel_token,
        );

        let res = agent.run("Exceed steps").await;
        assert!(
            matches!(res, TaskStatus::Failed(msg) if msg.contains("Exceeded maximum step limit"))
        );
    }

    #[tokio::test]
    async fn test_wall_clock_timeout_exceeded() {
        let executor = Arc::new(MockComputerUseExecutor::default());
        let mut model_client = MockComputerUseModelClient::new(vec![
            ComputerAction::Move { x: 10, y: 10 },
            ComputerAction::Move { x: 20, y: 20 },
        ]);
        model_client.hang_delay = Some(Duration::from_millis(100));
        let model = Arc::new(model_client);

        let config = ComputerUseAgentConfig {
            wall_clock_timeout: Duration::from_millis(50),
            ..Default::default()
        };

        let cancel_token = KillSwitchToken::new();
        let agent = ComputerUseAgentLoop::new(
            config,
            Arc::clone(&executor) as Arc<dyn ComputerUseExecutor>,
            Arc::clone(&model) as Arc<dyn ComputerUseModelClient>,
            None,
            cancel_token,
        );

        let res = agent.run("Timeout test").await;
        assert!(matches!(res, TaskStatus::Failed(msg) if msg.contains("timeout exceeded")));
    }

    #[tokio::test]
    async fn test_kill_switch_during_drag_releases_mouse() {
        let executor = Arc::new(MockComputerUseExecutor::default());
        let model = Arc::new(MockComputerUseModelClient::new(vec![
            ComputerAction::Drag {
                start_x: 100,
                start_y: 100,
                end_x: 600,
                end_y: 600,
            },
            ComputerAction::Move { x: 50, y: 50 },
        ]));

        let cancel_token = KillSwitchToken::new();
        let cancel_token_clone = cancel_token.clone();

        let agent = ComputerUseAgentLoop::new(
            ComputerUseAgentConfig::default(),
            Arc::clone(&executor) as Arc<dyn ComputerUseExecutor>,
            Arc::clone(&model) as Arc<dyn ComputerUseModelClient>,
            None,
            cancel_token,
        );

        // Cancel after step 1
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            cancel_token_clone.cancel();
        });

        let res = agent.run("Drag task").await;
        assert_eq!(res, TaskStatus::Cancelled);
        // Held mouse must be cleared and release_all_held_inputs called
        assert!(!executor.held_inputs.lock().unwrap().mouse_left_down);
        assert!(*executor.release_calls.lock().unwrap() >= 1);
    }

    #[tokio::test]
    async fn test_kill_switch_during_held_key_releases_key() {
        let executor = Arc::new(MockComputerUseExecutor::default());
        let model = Arc::new(MockComputerUseModelClient::new(vec![
            ComputerAction::KeyDown {
                key: "Shift".into(),
            },
            ComputerAction::Move { x: 10, y: 10 },
        ]));

        let cancel_token = KillSwitchToken::new();
        let cancel_token_clone = cancel_token.clone();

        let agent = ComputerUseAgentLoop::new(
            ComputerUseAgentConfig::default(),
            Arc::clone(&executor) as Arc<dyn ComputerUseExecutor>,
            Arc::clone(&model) as Arc<dyn ComputerUseModelClient>,
            None,
            cancel_token,
        );

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            cancel_token_clone.cancel();
        });

        let res = agent.run("Key down task").await;
        assert_eq!(res, TaskStatus::Cancelled);
        // Held keys must be cleared and release_all_held_inputs called
        assert!(executor.held_inputs.lock().unwrap().held_keys.is_empty());
        assert!(*executor.release_calls.lock().unwrap() >= 1);
    }

    #[tokio::test]
    async fn test_session_locked_stops_agent_loop() {
        let executor = Arc::new(MockComputerUseExecutor::default());
        let model = Arc::new(MockComputerUseModelClient::new(vec![
            ComputerAction::Click {
                x: 100,
                y: 100,
                button: MouseButton::Left,
            },
        ]));

        let inspector = Arc::new(MockPlatformSecurityInspector {
            session_locked: true,
            ..Default::default()
        });

        let cancel_token = KillSwitchToken::new();
        let agent = ComputerUseAgentLoop::new(
            ComputerUseAgentConfig::default(),
            Arc::clone(&executor) as Arc<dyn ComputerUseExecutor>,
            Arc::clone(&model) as Arc<dyn ComputerUseModelClient>,
            None,
            cancel_token,
        )
        .with_security_inspector(inspector);

        let res = agent.run("Locked session task").await;
        assert_eq!(res, TaskStatus::Failed("Session is locked".into()));
        assert!(*executor.release_calls.lock().unwrap() >= 1);
    }

    #[tokio::test]
    async fn test_elevated_foreground_or_secure_desktop_stops_agent_loop() {
        let executor = Arc::new(MockComputerUseExecutor::default());
        let model = Arc::new(MockComputerUseModelClient::new(vec![
            ComputerAction::Click {
                x: 100,
                y: 100,
                button: MouseButton::Left,
            },
        ]));

        let inspector = Arc::new(MockPlatformSecurityInspector {
            foreground_elevated: true,
            ..Default::default()
        });

        let cancel_token = KillSwitchToken::new();
        let agent = ComputerUseAgentLoop::new(
            ComputerUseAgentConfig::default(),
            Arc::clone(&executor) as Arc<dyn ComputerUseExecutor>,
            Arc::clone(&model) as Arc<dyn ComputerUseModelClient>,
            None,
            cancel_token,
        )
        .with_security_inspector(inspector);

        let res = agent.run("Elevated foreground task").await;
        assert_eq!(
            res,
            TaskStatus::Failed("Target window is elevated or secure desktop active".into())
        );
        assert!(*executor.release_calls.lock().unwrap() >= 1);
    }

    #[tokio::test]
    async fn test_password_field_detected_refuses_typing() {
        let executor = Arc::new(MockComputerUseExecutor::default());
        let model = Arc::new(MockComputerUseModelClient::new(vec![
            ComputerAction::TypeText {
                text: "ordinary_user_input".into(),
                press_enter: false,
            },
        ]));

        let inspector = Arc::new(MockPlatformSecurityInspector {
            password_field_detected: true,
            ..Default::default()
        });

        let cancel_token = KillSwitchToken::new();
        let agent = ComputerUseAgentLoop::new(
            ComputerUseAgentConfig::default(),
            Arc::clone(&executor) as Arc<dyn ComputerUseExecutor>,
            Arc::clone(&model) as Arc<dyn ComputerUseModelClient>,
            None,
            cancel_token,
        )
        .with_security_inspector(inspector);

        let res = agent.run("Password field task").await;
        assert!(matches!(res, TaskStatus::Failed(msg) if msg.contains("password field")));
        assert!(*executor.release_calls.lock().unwrap() >= 1);
    }

    #[tokio::test]
    async fn test_unverifiable_field_in_browser_refuses_typing() {
        let executor = Arc::new(MockComputerUseExecutor::default());
        let model = Arc::new(MockComputerUseModelClient::new(vec![
            ComputerAction::TypeText {
                text: "browser_search_term".into(),
                press_enter: false,
            },
        ]));

        let inspector = Arc::new(MockPlatformSecurityInspector {
            unverifiable_browser_field: true,
            ..Default::default()
        });

        let cancel_token = KillSwitchToken::new();
        let agent = ComputerUseAgentLoop::new(
            ComputerUseAgentConfig::default(),
            Arc::clone(&executor) as Arc<dyn ComputerUseExecutor>,
            Arc::clone(&model) as Arc<dyn ComputerUseModelClient>,
            None,
            cancel_token,
        )
        .with_security_inspector(inspector)
        .with_window_override("Google Chrome - New Tab", "chrome.exe");

        let res = agent.run("Browser search task").await;
        assert!(matches!(res, TaskStatus::Failed(msg) if msg.contains("Unverifiable browser")));
        assert!(*executor.release_calls.lock().unwrap() >= 1);
    }

    #[tokio::test]
    async fn test_audit_log_hygiene_never_records_typed_text_or_screenshots() {
        let executor = Arc::new(MockComputerUseExecutor::default());
        let sensitive_input = "sensitive_personal_data_xyz987";
        let model = Arc::new(MockComputerUseModelClient::new(vec![
            ComputerAction::TypeText {
                text: sensitive_input.into(),
                press_enter: true,
            },
            ComputerAction::Finish {
                result: Some("Done".into()),
            },
        ]));

        let cancel_token = KillSwitchToken::new();
        let agent = ComputerUseAgentLoop::new(
            ComputerUseAgentConfig::default(),
            Arc::clone(&executor) as Arc<dyn ComputerUseExecutor>,
            Arc::clone(&model) as Arc<dyn ComputerUseModelClient>,
            None,
            cancel_token,
        )
        .with_security_inspector(Arc::new(MockPlatformSecurityInspector::default()));

        let res = agent.run("Audit hygiene goal").await;
        assert_eq!(res, TaskStatus::Completed(Some("Done".into())));

        let logs = agent.get_audit_log();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].action_type, "type_text");

        // Serialize audit log to JSON string and verify absence of sensitive text and screenshot bytes
        let json_str = serde_json::to_string(&logs).unwrap();
        assert!(!json_str.contains(sensitive_input));
        assert!(!json_str.contains("jpeg_bytes"));
        assert!(!json_str.contains("data:image"));
    }
}
