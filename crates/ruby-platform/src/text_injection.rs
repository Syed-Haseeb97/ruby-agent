//! Text injection via SendInput Unicode for short text (<64 chars) and
//! clipboard paste (Ctrl+V) with state restore for long text.

use std::time::Duration;
use tracing::debug;
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_CONTROL,
};

use crate::PlatformError;

const CF_UNICODETEXT: u32 = 13;
const VK_V: VIRTUAL_KEY = VIRTUAL_KEY(0x56);

/// Injects text using SendInput with KEYEVENTF_UNICODE.
pub fn type_unicode_text(text: &str) -> Result<(), PlatformError> {
    if text.is_empty() {
        return Ok(());
    }

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

/// Pastes text via Ctrl+V, temporarily backing up and restoring clipboard contents.
pub fn paste_text_with_clipboard_restore(text: &str) -> Result<(), PlatformError> {
    unsafe {
        // 1. Open clipboard and snapshot previous Unicode text if any
        let mut previous_text: Option<String> = None;
        if OpenClipboard(HWND(std::ptr::null_mut())).is_ok() {
            let handle = GetClipboardData(CF_UNICODETEXT);
            if let Ok(h) = handle {
                if !h.0.is_null() {
                    let ptr = GlobalLock(HGLOBAL(h.0)) as *const u16;
                    if !ptr.is_null() {
                        let mut len = 0;
                        while *ptr.add(len) != 0 {
                            len += 1;
                        }
                        let slice = std::slice::from_raw_parts(ptr, len);
                        previous_text = Some(String::from_utf16_lossy(slice));
                        let _ = GlobalUnlock(HGLOBAL(h.0));
                    }
                }
            }
            let _ = CloseClipboard();
        }

        // 2. Set new text on clipboard
        set_clipboard_text(text)?;

        // 3. Send Ctrl+V input
        send_ctrl_v();

        // 4. Brief delay to allow target application to consume paste
        std::thread::sleep(Duration::from_millis(60));

        // 5. Restore previous clipboard content if there was one
        if let Some(prev) = previous_text {
            let _ = set_clipboard_text(&prev);
        }

        Ok(())
    }
}

unsafe fn set_clipboard_text(text: &str) -> Result<(), PlatformError> {
    if OpenClipboard(HWND(std::ptr::null_mut())).is_err() {
        return Err(PlatformError::CaptureFailed(
            "Cannot open clipboard".to_string(),
        ));
    }

    let _ = EmptyClipboard();

    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = utf16.len() * 2;

    let hglobal = match GlobalAlloc(GMEM_MOVEABLE, bytes) {
        Ok(h) => h,
        Err(_) => {
            let _ = CloseClipboard();
            return Err(PlatformError::CaptureFailed(
                "GlobalAlloc failed".to_string(),
            ));
        }
    };

    let dest = GlobalLock(hglobal) as *mut u16;
    if !dest.is_null() {
        std::ptr::copy_nonoverlapping(utf16.as_ptr(), dest, utf16.len());
        let _ = GlobalUnlock(hglobal);
        let _ = SetClipboardData(CF_UNICODETEXT, HANDLE(hglobal.0));
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
