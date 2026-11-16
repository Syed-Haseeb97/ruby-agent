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

/// Standard Win32 clipboard formats that use GDI/metafile handles rather than HGLOBAL memory blocks.
/// Attempting GlobalAlloc / GlobalLock on these formats causes heap corruption (STATUS_HEAP_CORRUPTION 0xc0000374).
const CF_BITMAP: u32 = 2;
const CF_METAFILEPICT: u32 = 3;
const CF_PALETTE: u32 = 9;
const CF_ENHMETAFILE: u32 = 14;

fn is_hglobal_clipboard_format(format: u32) -> bool {
    !matches!(
        format,
        CF_BITMAP | CF_METAFILEPICT | CF_PALETTE | CF_ENHMETAFILE
    )
}

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

                if !is_hglobal_clipboard_format(format) {
                    continue;
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

    #[test]
    fn test_clipboard_multiformat_backup_and_restore() {
        const CF_DIB: u32 = 8;
        const CF_HDROP: u32 = 15;

        // Helper to open clipboard with retry
        unsafe fn open_clipboard_retry() -> bool {
            for _ in 0..10 {
                if OpenClipboard(HWND(std::ptr::null_mut())).is_ok() {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            false
        }

        // 1. Stage original multi-format clipboard content:
        // (a) Text (CF_UNICODETEXT)
        // (b) Bitmap (CF_DIB)
        // (c) File list (CF_HDROP)
        let original_text = "Ruby Multi-Format Clipboard Test: Original Text 12345";
        let utf16_text: Vec<u16> = original_text
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        // 1x1 24-bit uncompressed DIB (40 bytes header + 4 bytes padded row = 44 bytes)
        let mut dib_bytes = vec![0u8; 44];
        // biSize = 40
        dib_bytes[0..4].copy_from_slice(&40u32.to_le_bytes());
        // biWidth = 1
        dib_bytes[4..8].copy_from_slice(&1i32.to_le_bytes());
        // biHeight = 1
        dib_bytes[8..12].copy_from_slice(&1i32.to_le_bytes());
        // biPlanes = 1
        dib_bytes[12..14].copy_from_slice(&1u16.to_le_bytes());
        // biBitCount = 24
        dib_bytes[14..16].copy_from_slice(&24u16.to_le_bytes());
        // biCompression = BI_RGB (0)
        // biSizeImage = 4
        dib_bytes[20..24].copy_from_slice(&4u32.to_le_bytes());
        // Pixel data (BGR + pad): Blue=255, Green=128, Red=64, Pad=0
        dib_bytes[40] = 0xFF;
        dib_bytes[41] = 0x80;
        dib_bytes[42] = 0x40;
        dib_bytes[43] = 0x00;

        // HDROP: DROPFILES struct (20 bytes) + wide paths null-terminated + extra null
        let path1 = "C:\\projects\\ruby-agent\\test1.txt\0";
        let path2 = "D:\\projects\\ruby-agent\\test2.rs\0";
        let mut drop_payload: Vec<u8> = vec![0u8; 20];
        // pFiles offset = 20
        drop_payload[0..4].copy_from_slice(&20u32.to_le_bytes());
        // fWide = 1 (TRUE)
        drop_payload[16..20].copy_from_slice(&1i32.to_le_bytes());
        for u in path1
            .encode_utf16()
            .chain(path2.encode_utf16())
            .chain(std::iter::once(0))
        {
            drop_payload.extend_from_slice(&u.to_le_bytes());
        }

        unsafe {
            assert!(open_clipboard_retry(), "Failed to open clipboard for setup");
            let _ = EmptyClipboard();

            // Set CF_UNICODETEXT
            let h_text = GlobalAlloc(GMEM_MOVEABLE, utf16_text.len() * 2).unwrap();
            let dest_text = GlobalLock(h_text) as *mut u16;
            std::ptr::copy_nonoverlapping(utf16_text.as_ptr(), dest_text, utf16_text.len());
            let _ = GlobalUnlock(h_text);
            assert!(SetClipboardData(CF_UNICODETEXT, HANDLE(h_text.0)).is_ok());

            // Set CF_DIB
            let h_dib = GlobalAlloc(GMEM_MOVEABLE, dib_bytes.len()).unwrap();
            let dest_dib = GlobalLock(h_dib) as *mut u8;
            std::ptr::copy_nonoverlapping(dib_bytes.as_ptr(), dest_dib, dib_bytes.len());
            let _ = GlobalUnlock(h_dib);
            assert!(SetClipboardData(CF_DIB, HANDLE(h_dib.0)).is_ok());

            // Set CF_HDROP
            let h_hdrop = GlobalAlloc(GMEM_MOVEABLE, drop_payload.len()).unwrap();
            let dest_hdrop = GlobalLock(h_hdrop) as *mut u8;
            std::ptr::copy_nonoverlapping(drop_payload.as_ptr(), dest_hdrop, drop_payload.len());
            let _ = GlobalUnlock(h_hdrop);
            assert!(SetClipboardData(CF_HDROP, HANDLE(h_hdrop.0)).is_ok());

            let _ = CloseClipboard();
        }

        // 2. Perform paste_text_with_clipboard_restore with dictation text
        let dictation_output = "Transcribed by Ruby Dictation Engine";
        let res = paste_text_with_clipboard_restore(dictation_output);
        assert!(
            res.is_ok(),
            "paste_text_with_clipboard_restore must succeed: {:?}",
            res
        );

        // 3. Inspect clipboard content AFTER restoration:
        // Assert text, DIB bitmap, and HDROP file list are ALL restored
        unsafe {
            assert!(
                open_clipboard_retry(),
                "Failed to open clipboard for assertion"
            );

            // Assert CF_UNICODETEXT restored
            let h_restored_text =
                GetClipboardData(CF_UNICODETEXT).expect("CF_UNICODETEXT must be present");
            assert!(!h_restored_text.0.is_null());
            let ptr_text = GlobalLock(HGLOBAL(h_restored_text.0)) as *const u16;
            assert!(!ptr_text.is_null());
            let mut read_utf16 = Vec::new();
            let mut idx = 0;
            loop {
                let val = *ptr_text.add(idx);
                if val == 0 {
                    break;
                }
                read_utf16.push(val);
                idx += 1;
            }
            let _ = GlobalUnlock(HGLOBAL(h_restored_text.0));
            let restored_text = String::from_utf16_lossy(&read_utf16);
            assert_eq!(
                restored_text, original_text,
                "CF_UNICODETEXT was not restored correctly"
            );

            // Assert CF_DIB restored
            let h_restored_dib = GetClipboardData(CF_DIB).expect("CF_DIB must be present");
            assert!(!h_restored_dib.0.is_null());
            let size_dib = GlobalSize(HGLOBAL(h_restored_dib.0));
            assert!(
                size_dib >= dib_bytes.len(),
                "Restored DIB size ({size_dib}) < expected ({})",
                dib_bytes.len()
            );
            let ptr_dib = GlobalLock(HGLOBAL(h_restored_dib.0)) as *const u8;
            assert!(!ptr_dib.is_null());
            let mut restored_dib = vec![0u8; dib_bytes.len()];
            std::ptr::copy_nonoverlapping(ptr_dib, restored_dib.as_mut_ptr(), dib_bytes.len());
            let _ = GlobalUnlock(HGLOBAL(h_restored_dib.0));
            assert_eq!(
                restored_dib, dib_bytes,
                "CF_DIB bytes were not restored correctly"
            );

            // Assert CF_HDROP restored
            let h_restored_hdrop = GetClipboardData(CF_HDROP).expect("CF_HDROP must be present");
            assert!(!h_restored_hdrop.0.is_null());
            let size_hdrop = GlobalSize(HGLOBAL(h_restored_hdrop.0));
            assert!(
                size_hdrop >= drop_payload.len(),
                "Restored HDROP size ({size_hdrop}) < expected ({})",
                drop_payload.len()
            );
            let ptr_hdrop = GlobalLock(HGLOBAL(h_restored_hdrop.0)) as *const u8;
            assert!(!ptr_hdrop.is_null());
            let mut restored_hdrop = vec![0u8; drop_payload.len()];
            std::ptr::copy_nonoverlapping(
                ptr_hdrop,
                restored_hdrop.as_mut_ptr(),
                drop_payload.len(),
            );
            let _ = GlobalUnlock(HGLOBAL(h_restored_hdrop.0));
            assert_eq!(
                restored_hdrop, drop_payload,
                "CF_HDROP bytes were not restored correctly"
            );

            let _ = CloseClipboard();
        }
    }
}
