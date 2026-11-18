//! Screen capture abstraction and service for remote visual verification (Phase R2).

use async_trait::async_trait;
use ruby_actions::computer_use::agent_loop::PlatformSecurityInspector;
use ruby_actions::computer_use::executor::ComputerUseExecutor;
use ruby_policy::computer_policy::is_window_denylisted_with_custom;
use std::sync::Arc;

/// Represents a successfully captured desktop screenshot in memory.
#[derive(Debug, Clone)]
pub struct CapturedScreen {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub mime_type: String,
}

/// Interface for capturing remote screen with security and policy guards.
#[async_trait]
pub trait RemoteScreenCapturer: Send + Sync {
    /// Captures the desktop screen, enforcing security policies (session lock,
    /// elevation/secure desktop, and window denylists).
    async fn capture_screen(&self) -> Result<CapturedScreen, String>;
}

/// Production implementation of RemoteScreenCapturer wrapping ComputerUseExecutor
/// and PlatformSecurityInspector with denylist and session validation.
pub struct DefaultRemoteScreenCapturer {
    executor: Arc<dyn ComputerUseExecutor>,
    inspector: Arc<dyn PlatformSecurityInspector>,
    denylist: Vec<String>,
    window_override: Arc<std::sync::Mutex<Option<(String, String)>>>,
}

impl DefaultRemoteScreenCapturer {
    pub fn new(
        executor: Arc<dyn ComputerUseExecutor>,
        inspector: Arc<dyn PlatformSecurityInspector>,
        denylist: Vec<String>,
    ) -> Self {
        Self {
            executor,
            inspector,
            denylist,
            window_override: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Sets an explicit window title and process override for hermetic testing.
    pub fn set_window_override(&self, title: &str, proc_name: &str) {
        *self.window_override.lock().unwrap() = Some((title.to_string(), proc_name.to_string()));
    }

    /// Clears any window override.
    pub fn clear_window_override(&self) {
        *self.window_override.lock().unwrap() = None;
    }

    fn inspect_active_window(&self) -> (String, String) {
        if let Some(pair) = self.window_override.lock().unwrap().clone() {
            return pair;
        }
        ruby_actions::computer_use::get_active_window_info()
    }
}

#[async_trait]
impl RemoteScreenCapturer for DefaultRemoteScreenCapturer {
    async fn capture_screen(&self) -> Result<CapturedScreen, String> {
        // 1. Session lock check
        if self.inspector.is_session_locked().await {
            return Err("Windows session is locked".into());
        }

        // 2. Elevated / secure desktop check
        if self
            .inspector
            .is_foreground_elevated_or_secure_desktop()
            .await
        {
            return Err("Secure desktop or elevated process active".into());
        }

        // 3. Denylist check on foreground window
        let (title, proc_name) = self.inspect_active_window();
        if is_window_denylisted_with_custom(&title, &proc_name, &self.denylist) {
            return Err(format!(
                "Foreground window \"{title}\" is protected by security denylist"
            ));
        }

        // 4. In-memory screen capture
        let capture = self
            .executor
            .capture_screen()
            .await
            .map_err(|e| format!("Capture failed: {e}"))?;

        Ok(CapturedScreen {
            width: capture.width,
            height: capture.height,
            data: capture.jpeg_bytes,
            mime_type: "image/jpeg".into(),
        })
    }
}

/// Mock implementation of RemoteScreenCapturer for offline testing.
#[derive(Clone)]
pub struct MockRemoteScreenCapturer {
    pub session_locked: bool,
    pub elevated_or_secure_desktop: bool,
    pub denylisted_window: bool,
    pub mock_error: Option<String>,
    pub mock_bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Default for MockRemoteScreenCapturer {
    fn default() -> Self {
        Self {
            session_locked: false,
            elevated_or_secure_desktop: false,
            denylisted_window: false,
            mock_error: None,
            mock_bytes: vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46], // JPEG magic header
            width: 1920,
            height: 1080,
        }
    }
}

#[async_trait]
impl RemoteScreenCapturer for MockRemoteScreenCapturer {
    async fn capture_screen(&self) -> Result<CapturedScreen, String> {
        if self.session_locked {
            return Err("Windows session is locked".into());
        }
        if self.elevated_or_secure_desktop {
            return Err("Secure desktop or elevated process active".into());
        }
        if self.denylisted_window {
            return Err("Foreground window is protected by security denylist".into());
        }
        if let Some(ref err) = self.mock_error {
            return Err(err.clone());
        }
        Ok(CapturedScreen {
            width: self.width,
            height: self.height,
            data: self.mock_bytes.clone(),
            mime_type: "image/jpeg".into(),
        })
    }
}
