//! Low-Level Keyboard Hook thread, message loop, and hotkey action dispatcher.
//! Runs on a dedicated OS thread, ignores LLKHF_INJECTED events, injects VK_NONAME
//! to suppress the Windows Start Menu, and feeds the deterministic state machine.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use tracing::{error, info, trace, warn};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VK_NONAME,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::hook_types::{HotKeyAction, RawKeyEvent};
use crate::hotkey_machine::HotKeyStateMachine;

// Global hook channels and hook handle
static mut EVENT_SENDER: Option<Sender<RawKeyEvent>> = None;
static mut HOOK_HANDLE: Option<HHOOK> = None;
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);

/// Win32 low-level keyboard hook callback procedure.
/// Strict rule: does NO heavy work in the callback beyond posting to the channel.
unsafe extern "system" fn low_level_keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 {
        let msg = w_param.0 as u32;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;

        if is_up || is_down {
            let kbd = *(l_param.0 as *const KBDLLHOOKSTRUCT);
            let is_injected = (kbd.flags.0 & LLKHF_INJECTED.0) != 0;

            // Only forward non-injected events or ignore in callback
            if !is_injected {
                if let Some(ref sender) = EVENT_SENDER {
                    let _ = sender.try_send(RawKeyEvent {
                        vk_code: kbd.vkCode as u16,
                        is_up,
                        is_injected: false,
                        timestamp_ms: 0, // Assigned by receiver based on monotonic clock
                    });
                }
            }
        }
    }

    CallNextHookEx(HOOK_HANDLE.unwrap_or_default(), n_code, w_param, l_param)
}

/// Injects unassigned VK_NONAME (0xFC) to suppress the Windows Start Menu.
pub fn inject_start_menu_suppression() {
    let inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_NONAME,
                    wScan: 0,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_NONAME,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
    ];

    unsafe {
        let count = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if count != 2 {
            warn!("Start menu suppression SendInput returned count {}", count);
        } else {
            trace!("Injected VK_NONAME Start Menu suppression key");
        }
    }
}

pub struct KeyboardHookHandle {
    hook_thread: Option<JoinHandle<()>>,
    worker_thread: Option<JoinHandle<()>>,
    action_receiver: Receiver<HotKeyAction>,
    is_running: Arc<AtomicBool>,
}

impl KeyboardHookHandle {
    /// Start the keyboard hook thread, message loop, and hotkey processing worker.
    pub fn start() -> Result<Self, windows::core::Error> {
        let (raw_tx, raw_rx) = crossbeam_channel::unbounded::<RawKeyEvent>();
        let (action_tx, action_rx) = crossbeam_channel::unbounded::<HotKeyAction>();
        let is_running = Arc::new(AtomicBool::new(true));

        unsafe {
            EVENT_SENDER = Some(raw_tx);
        }

        let is_running_clone = Arc::clone(&is_running);
        let hook_ready = Arc::new(AtomicBool::new(false));
        let hook_ready_clone = Arc::clone(&hook_ready);

        // 1. Dedicated OS thread for SetWindowsHookExW and GetMessageW loop
        let hook_thread = std::thread::Builder::new()
            .name("ruby-kb-hook-thread".to_string())
            .spawn(move || {
                let thread_id = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
                HOOK_THREAD_ID.store(thread_id, Ordering::SeqCst);

                unsafe {
                    let hook =
                        SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), None, 0);

                    match hook {
                        Ok(h) => {
                            HOOK_HANDLE = Some(h);
                            info!(
                                "Low-level keyboard hook installed successfully (thread: {})",
                                thread_id
                            );
                            hook_ready_clone.store(true, Ordering::SeqCst);

                            let mut msg = MSG::default();
                            while is_running_clone.load(Ordering::SeqCst) {
                                let res = GetMessageW(&mut msg, HWND(std::ptr::null_mut()), 0, 0);
                                if res.0 == 0 || res.0 == -1 {
                                    break;
                                }
                                let _ = TranslateMessage(&msg);
                                DispatchMessageW(&msg);
                            }

                            let _ = UnhookWindowsHookEx(h);
                            HOOK_HANDLE = None;
                            info!("Low-level keyboard hook uninstalled");
                        }
                        Err(e) => {
                            error!("Failed to install low-level keyboard hook: {:?}", e);
                            hook_ready_clone.store(true, Ordering::SeqCst);
                        }
                    }
                }
            })
            .expect("Failed to spawn keyboard hook thread");

        // Wait briefly for hook thread to initialize
        let start_time = Instant::now();
        while !hook_ready.load(Ordering::SeqCst)
            && start_time.elapsed() < Duration::from_millis(500)
        {
            std::thread::sleep(Duration::from_millis(10));
        }

        // 2. Worker thread processing raw key events and driving the state machine
        let is_running_worker = Arc::clone(&is_running);
        let worker_thread = std::thread::Builder::new()
            .name("ruby-kb-worker-thread".to_string())
            .spawn(move || {
                let mut machine = HotKeyStateMachine::default();
                let clock = Instant::now();

                while is_running_worker.load(Ordering::SeqCst) {
                    let now_ms = clock.elapsed().as_millis() as u64;

                    // Drain all pending raw key events
                    while let Ok(mut event) = raw_rx.try_recv() {
                        event.timestamp_ms = now_ms;

                        // Start-menu suppression: if Win key was pressed down, inject VK_NONAME
                        if event.is_win() && !event.is_up {
                            inject_start_menu_suppression();
                        }

                        let actions = machine.feed_key(event, now_ms);
                        for action in actions {
                            let _ = action_tx.send(action);
                        }
                    }

                    // Periodic tick to check hold duration or double-tap expiration
                    let tick_actions = machine.tick(now_ms);
                    for action in tick_actions {
                        let _ = action_tx.send(action);
                    }

                    // Sleep small slice (10ms) to conserve CPU
                    std::thread::sleep(Duration::from_millis(10));
                }
            })
            .expect("Failed to spawn keyboard worker thread");

        Ok(Self {
            hook_thread: Some(hook_thread),
            worker_thread: Some(worker_thread),
            action_receiver: action_rx,
            is_running,
        })
    }

    pub fn receiver(&self) -> &Receiver<HotKeyAction> {
        &self.action_receiver
    }
}

impl Drop for KeyboardHookHandle {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);

        let thread_id = HOOK_THREAD_ID.load(Ordering::SeqCst);
        if thread_id != 0 {
            unsafe {
                let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }

        if let Some(handle) = self.hook_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.worker_thread.take() {
            let _ = handle.join();
        }
    }
}
