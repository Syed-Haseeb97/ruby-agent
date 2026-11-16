//! Process elevation (UIPI guard) and password field detection for text insertion.
//! Implements Win32 controls, styles, and UI Automation (IUIAutomation Element CurrentIsPassword).
//! Fails closed (refuses insertion) if the security check cannot be completed.

use tracing::{debug, warn};
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::ProcessStatus::GetProcessImageFileNameW;
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowLongW, GetWindowThreadProcessId,
    SendMessageW, GUITHREADINFO, GWL_STYLE,
};

use crate::PlatformError;

const EM_GETPASSWORDCHAR: u32 = 0x00D2;
const ES_PASSWORD: i32 = 0x0020;

/// Check if a process token has elevated privileges.
unsafe fn is_token_elevated(token: HANDLE) -> bool {
    let mut elevation = TOKEN_ELEVATION::default();
    let mut return_length = 0u32;
    let res = GetTokenInformation(
        token,
        TokenElevation,
        Some(&mut elevation as *mut _ as *mut _),
        std::mem::size_of::<TOKEN_ELEVATION>() as u32,
        &mut return_length,
    );

    if res.is_ok() {
        elevation.TokenIsElevated != 0
    } else {
        false
    }
}

/// Check if current Ruby agent process is running elevated.
pub fn is_current_process_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_ok() {
            let elevated = is_token_elevated(token);
            let _ = CloseHandle(token);
            elevated
        } else {
            false
        }
    }
}

/// Check if target window's owning process is elevated.
/// Under Windows UIPI, an unelevated process cannot inject input into an elevated window.
pub fn is_target_window_elevated(hwnd: HWND) -> Result<bool, PlatformError> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return Ok(false);
        }

        let process_handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(_e) => {
                // If OpenProcess fails with ACCESS_DENIED, the target process is elevated or protected (SYSTEM/Admin).
                return Ok(true);
            }
        };

        let mut token = HANDLE::default();
        let is_elevated = if OpenProcessToken(process_handle, TOKEN_QUERY, &mut token).is_ok() {
            let elevated = is_token_elevated(token);
            let _ = CloseHandle(token);
            elevated
        } else {
            // Cannot query token -> protected/elevated
            true
        };

        let _ = CloseHandle(process_handle);
        Ok(is_elevated)
    }
}

/// Get process executable name for foreground window (for tone adaptation).
pub fn get_foreground_process_name() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }

        let process_handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let len = GetProcessImageFileNameW(process_handle, &mut buf);
        let _ = CloseHandle(process_handle);

        if len > 0 {
            let full_path = String::from_utf16_lossy(&buf[..len as usize]);
            let file_name = full_path.rsplit('\\').next().unwrap_or(&full_path);
            Some(file_name.to_lowercase())
        } else {
            None
        }
    }
}

/// Checks the focused element via Windows UI Automation (IUIAutomation Element CurrentIsPassword).
/// Covers Chromium/Edge web pages and Electron applications.
/// Strict rule: Fails closed (returns Err) if the element cannot be inspected or queried.
pub fn is_focused_element_password_uia() -> Result<bool, PlatformError> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let uia: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
        {
            Ok(u) => u,
            Err(e) => {
                warn!("UI Automation CoCreateInstance failed: {:?}", e);
                return Err(PlatformError::SecurityCheckFailed(format!(
                    "UI Automation initialization error: {:?}",
                    e
                )));
            }
        };

        let focused = match uia.GetFocusedElement() {
            Ok(el) => el,
            Err(e) => {
                debug!("UI Automation GetFocusedElement returned: {:?}", e);
                // If there is no focused element or inaccessible, fail closed
                return Err(PlatformError::SecurityCheckFailed(format!(
                    "Cannot retrieve focused UI Automation element: {:?}",
                    e
                )));
            }
        };

        let is_pw = match focused.CurrentIsPassword() {
            Ok(b) => b.as_bool(),
            Err(e) => {
                warn!("UI Automation CurrentIsPassword failed: {:?}", e);
                return Err(PlatformError::SecurityCheckFailed(format!(
                    "Cannot query CurrentIsPassword: {:?}",
                    e
                )));
            }
        };

        Ok(is_pw)
    }
}

/// Inspect foreground window and focused control using Win32 and UI Automation.
/// Returns Ok(()) if safe, or Err(PlatformError) if a password field is detected
/// or if security checks fail closed.
pub fn check_password_guardrails() -> Result<(), PlatformError> {
    unsafe {
        let hwnd_foreground = GetForegroundWindow();
        if hwnd_foreground.0.is_null() {
            return Err(PlatformError::SecurityCheckFailed(
                "No foreground window active".to_string(),
            ));
        }

        let thread_id = GetWindowThreadProcessId(hwnd_foreground, None);
        let mut gui_info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };

        let focused_hwnd = if GetGUIThreadInfo(thread_id, &mut gui_info).is_ok()
            && !gui_info.hwndFocus.0.is_null()
        {
            gui_info.hwndFocus
        } else {
            hwnd_foreground
        };

        // 1. Check Win32 window style for ES_PASSWORD
        let style = GetWindowLongW(focused_hwnd, GWL_STYLE);
        if (style & ES_PASSWORD) != 0 {
            return Err(PlatformError::PasswordFieldDetected);
        }

        // 2. Check EM_GETPASSWORDCHAR
        let pw_char = SendMessageW(focused_hwnd, EM_GETPASSWORDCHAR, WPARAM(0), LPARAM(0));
        if pw_char.0 != 0 {
            return Err(PlatformError::PasswordFieldDetected);
        }

        // 3. Check class name for known password boxes
        let mut class_buf = [0u16; 128];
        let class_len = GetClassNameW(focused_hwnd, &mut class_buf);
        if class_len > 0 {
            let class_name =
                String::from_utf16_lossy(&class_buf[..class_len as usize]).to_lowercase();
            if class_name.contains("password") {
                return Err(PlatformError::PasswordFieldDetected);
            }
        }

        // 4. Check UI Automation IsPassword (covers Edge/Chrome/Electron)
        // If UIA reports password, refuse!
        // If UIA returns an error (fail-closed requirement), refuse!
        match is_focused_element_password_uia() {
            Ok(true) => return Err(PlatformError::PasswordFieldDetected),
            Ok(false) => {}
            Err(e) => {
                debug!("UIA check failed closed: {:?}", e);
                // When in headless testing or automated shell without desktop focus,
                // fallback to Win32 checks if Win32 verified non-password, but in production fail closed.
                return Err(e);
            }
        }

        Ok(())
    }
}
