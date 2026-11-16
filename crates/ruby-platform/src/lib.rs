//! Windows OS Automation & Native Integration for Ruby Assistant.
//! Provides SendInput, Windows.Graphics.Capture, low-level keyboard hook,
//! clipboard capture/restore, and the DesktopController trait with mock.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WINDOW_DISPLAY_AFFINITY};

pub const VK_NONAME_VALUE: u16 = 0xFC;
pub const WDA_EXCLUDEFROMCAPTURE_VALUE: u32 = 0x00000011;

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("Win32 error: {0}")]
    Win32(#[from] windows::core::Error),
    #[error("Target window is running elevated (UIPI limitation)")]
    ElevatedTargetBlocked,
    #[error("Focused control is a password field; typing aborted")]
    PasswordFieldDetected,
    #[error("Screen capture failed: {0}")]
    CaptureFailed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenSize {
    pub width: u32,
    pub height: u32,
}

pub trait DesktopController: Send + Sync {
    fn screen_size(&self) -> Result<ScreenSize, PlatformError>;
    fn capture_primary_screen(&self) -> Result<Vec<u8>, PlatformError>;
    fn click(&self, x: u32, y: u32) -> Result<(), PlatformError>;
    fn double_click(&self, x: u32, y: u32) -> Result<(), PlatformError>;
    fn type_text(&self, text: &str) -> Result<(), PlatformError>;
    fn paste_text_with_restore(&self, text: &str) -> Result<(), PlatformError>;
    fn is_kill_switch_active(&self) -> bool;
}

/// Mock DesktopController for offline unit and integration testing.
pub struct MockDesktopController {
    pub screen_size: ScreenSize,
    pub kill_switch: Arc<AtomicBool>,
}

impl Default for MockDesktopController {
    fn default() -> Self {
        Self {
            screen_size: ScreenSize {
                width: 1920,
                height: 1080,
            },
            kill_switch: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl DesktopController for MockDesktopController {
    fn screen_size(&self) -> Result<ScreenSize, PlatformError> {
        Ok(self.screen_size)
    }

    fn capture_primary_screen(&self) -> Result<Vec<u8>, PlatformError> {
        Ok(vec![0xFF, 0xD8, 0xFF, 0xE0]) // Mock JPEG magic header
    }

    fn click(&self, _x: u32, _y: u32) -> Result<(), PlatformError> {
        Ok(())
    }

    fn double_click(&self, _x: u32, _y: u32) -> Result<(), PlatformError> {
        Ok(())
    }

    fn type_text(&self, _text: &str) -> Result<(), PlatformError> {
        Ok(())
    }

    fn paste_text_with_restore(&self, _text: &str) -> Result<(), PlatformError> {
        Ok(())
    }

    fn is_kill_switch_active(&self) -> bool {
        self.kill_switch.load(Ordering::SeqCst)
    }
}

/// Helper to exclude a window from screen captures.
pub fn exclude_window_from_capture(hwnd: HWND) -> Result<(), PlatformError> {
    unsafe {
        SetWindowDisplayAffinity(hwnd, WINDOW_DISPLAY_AFFINITY(WDA_EXCLUDEFROMCAPTURE_VALUE))?;
    }
    Ok(())
}
