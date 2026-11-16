//! Text injection via SendInput Unicode for short text (<64 chars) and
//! comprehensive clipboard paste (Ctrl+V) with full multi-format backup/restore,
//! Win+V clipboard history suppression, and modifier physical release synchronization.

use std::time::{Duration, Instant};
use tracing::{debug, warn};
use windows::core::w;
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_RWIN,
};

use crate::PlatformError;

const CF_UNICODETEXT: u32 = 13;
const VK_V: VIRTUAL_KEY = VIRTUAL_KEY(0x56);

#[derive(Debug, Clone)]
struct ClipboardSnapshotEntry {
    format: u32,
    bytes: Vec<u8>,
}

/// Waits until Ctrl and Win modifier keys are physically released before injecting input.
/// Prevents Windows from combining injected keystrokes with physically held modifier keys.
pub fn wait_for_modifiers_physically_released(timeout: Duration) -> Result<(), PlatformError> {
    let start = Instant::now();

    loop {
        let ctrl_down = unsafe { (GetAsyncKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 };
        let lwin_down = unsafe { (GetAsyncKeyState(VK_LWIN.0 as i32) as u16 & 0x8000) != 0 };
        let rwin_down = unsafe { (GetAsyncKeyState(VK_RWIN.0 as i32) as u16 & 0x8000) != 0 };

        if !ctrl_down && !lwin_down && !rwin_down {
            return Ok(());
        }

        if start.elapsed() > timeout {
            warn!(
                ctrl_down,
                lwin_down,
                rwin_down,
                "Timeout waiting for physical Ctrl/Win release; aborting insertion to prevent key collision"
            );
            return Err(PlatformError::ModifierReleaseTimeout);
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Injects text using SendInput with KEYEVENTF_UNICODE.
pub fn type_unicode_text(text: &str) -> Result<(), PlatformError> {
    if text.is_empty() {
        return Ok(());
    }

    // Wait until user's hands have released Ctrl+Win
    wait_for_modifiers_physically_released(Duration::from_millis(1500))?;

    let mut inputs = Vec::new();
    for code_unit in text.encode_utf16() {
        // Key down
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: code_unit,
                    dwFlags: KEYEVENTF_UNICODE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
        // Key up
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: code_unit,
                    dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
    }

    unsafe {
        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent != inputs.len() as u32 {
            debug!("SendInput injected {} of {} events", sent, inputs.len());
        }
    }

    Ok(())
}

/// Pastes text via Ctrl+V with full multi-format clipboard backup and restore,
/// and registers clipboard-history suppression so Win+V does not record the paste.
pub fn paste_text_with_clipboard_restore(text: &str) -> Result<(), PlatformError> {
    // 1. Wait until user has released Ctrl and Win
    wait_for_modifiers_physically_released(Duration::from_millis(1500))?;

    unsafe {
        // 2. Snapshot ALL existing clipboard formats
        let mut snapshot: Vec<ClipboardSnapshotEntry> = Vec::new();

        if OpenClipboard(HWND(std::ptr::null_mut())).is_ok() {
            let mut format = 0u32;
            loop {
                format = EnumClipboardFormats(format);
                if format == 0 {
                    break;
                }

                if let Ok(handle) = GetClipboardData(format) {
                    if !handle.0.is_null() {
                        let size = GlobalSize(HGLOBAL(handle.0));
                        if size > 0 {
                            let ptr = GlobalLock(HGLOBAL(handle.0)) as *const u8;
                            if !ptr.is_null() {
                                let mut data = vec![0u8; size];
                                std::ptr::copy_nonoverlapping(ptr, data.as_mut_ptr(), size);
                                let _ = GlobalUnlock(HGLOBAL(handle.0));
                                snapshot.push(ClipboardSnapshotEntry {
                                    format,
                                    bytes: data,
                                });
                            }
                        }
                    }
                }
            }
            let _ = CloseClipboard();
        }

        // 3. Set new text on clipboard with Win+V history exclusion flags
        set_clipboard_text_with_history_exclusion(text)?;

        // 4. Send Ctrl+V input
        send_ctrl_v();

        // 5. Brief delay to allow target application to consume paste
        std::thread::sleep(Duration::from_millis(80));

        // 6. Restore ALL previous clipboard formats exactly as they were
        if OpenClipboard(HWND(std::ptr::null_mut())).is_ok() {
            let _ = EmptyClipboard();

            for entry in snapshot {
                if let Ok(hglobal) = GlobalAlloc(GMEM_MOVEABLE, entry.bytes.len()) {
                    let dest = GlobalLock(hglobal) as *mut u8;
                    if !dest.is_null() {
                        std::ptr::copy_nonoverlapping(
                            entry.bytes.as_ptr(),
                            dest,
                            entry.bytes.len(),
                        );
                        let _ = GlobalUnlock(hglobal);
                        let _ = SetClipboardData(entry.format, HANDLE(hglobal.0));
                    }
                }
            }

            let _ = CloseClipboard();
        }

        Ok(())
    }
}

/// Sets text on clipboard and adds clipboard history suppression formats:
/// - ExcludeClipboardContentFromMonitorProcessing
/// - CanIncludeInClipboardHistory = 0
unsafe fn set_clipboard_text_with_history_exclusion(text: &str) -> Result<(), PlatformError> {
    if OpenClipboard(HWND(std::ptr::null_mut())).is_err() {
        return Err(PlatformError::CaptureFailed(
            "Cannot open clipboard".to_string(),
        ));
    }

    let _ = EmptyClipboard();

    // 1. Set CF_UNICODETEXT
    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = utf16.len() * 2;

    let h_text = match GlobalAlloc(GMEM_MOVEABLE, bytes) {
        Ok(h) => h,
        Err(_) => {
            let _ = CloseClipboard();
            return Err(PlatformError::CaptureFailed(
                "GlobalAlloc failed".to_string(),
            ));
        }
    };

    let dest = GlobalLock(h_text) as *mut u16;
    if !dest.is_null() {
        std::ptr::copy_nonoverlapping(utf16.as_ptr(), dest, utf16.len());
        let _ = GlobalUnlock(h_text);
        let _ = SetClipboardData(CF_UNICODETEXT, HANDLE(h_text.0));
    }

    // 2. Add Win+V Clipboard History Exclusion Formats:
    // "ExcludeClipboardContentFromMonitorProcessing"
    let fmt_exclude = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
    if fmt_exclude != 0 {
        if let Ok(h) = GlobalAlloc(GMEM_MOVEABLE, 4) {
            let ptr = GlobalLock(h) as *mut u32;
            if !ptr.is_null() {
                *ptr = 0;
                let _ = GlobalUnlock(h);
                let _ = SetClipboardData(fmt_exclude, HANDLE(h.0));
            }
        }
    }

    // "CanIncludeInClipboardHistory" (DWORD 0)
    let fmt_no_history = RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory"));
    if fmt_no_history != 0 {
        if let Ok(h) = GlobalAlloc(GMEM_MOVEABLE, 4) {
            let ptr = GlobalLock(h) as *mut u32;
            if !ptr.is_null() {
                *ptr = 0;
                let _ = GlobalUnlock(h);
                let _ = SetClipboardData(fmt_no_history, HANDLE(h.0));
            }
        }
    }

    let _ = CloseClipboard();
    Ok(())
}

fn send_ctrl_v() {
    let inputs = [
        // Ctrl down
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_CONTROL,
                    wScan: 0,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
        // V down
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_V,
                    wScan: 0,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
        // V up
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_V,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
        // Ctrl up
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_CONTROL,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
    ];

    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clipboard_history_exclusion_formats_registered() {
        let fmt_exclude =
            unsafe { RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing")) };
        let fmt_no_history =
            unsafe { RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory")) };

        assert_ne!(
            fmt_exclude, 0,
            "ExcludeClipboardContentFromMonitorProcessing must register successfully"
        );
        assert_ne!(
            fmt_no_history, 0,
            "CanIncludeInClipboardHistory must register successfully"
        );
    }

    #[test]
    fn test_wait_for_modifiers_released_returns_ok_when_not_pressed() {
        // When running in headless tests where no physical keys are held, function should return Ok(()) immediately
        let res = wait_for_modifiers_physically_released(Duration::from_millis(50));
        assert!(res.is_ok());
    }
}
