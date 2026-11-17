//! Ruby Desktop Tauri 2.0 Application Shell.
//! Coordinates the transparent floating Liquid Orb window,
//! the Editorial Control Console, typed IPC, and Win32 window properties.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, Window};
use windows::Win32::{
    Foundation::HWND,
    Graphics::Dwm::{
        DwmSetWindowAttribute, DWMNCRENDERINGPOLICY, DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY,
    },
    UI::WindowsAndMessaging::{
        GetWindowLongW, SetWindowDisplayAffinity, SetWindowLongW, GWL_EXSTYLE,
        WINDOW_DISPLAY_AFFINITY, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
    },
};

use ruby_core::{OperationalState, OrbVisualState};
use ruby_dictation::SystemDictationEngine;
use ruby_gemini::GeminiDictationClient;
use ruby_platform::{HotKeyAction, KeyboardHookHandle, WindowsDesktopController};
use ruby_remote::{ChannelAdapter, NoopCommandHandler, TelegramAdapter};
use ruby_voice::CpalAudioCapture;

pub mod commands;

pub struct AppState {
    pub orb_state: Mutex<OrbVisualState>,
    pub seq_counter: AtomicU64,
    pub click_through: AtomicBool,
    pub remote_adapter: Arc<TelegramAdapter>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SequencedOrbEvent {
    pub seq: u64,
    pub state: OrbVisualState,
}

pub fn configure_orb_window_win32(window: &Window) {
    if let Ok(hwnd_val) = window.hwnd() {
        let hwnd = HWND(hwnd_val.0 as *mut _);
        unsafe {
            // Apply WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED, and strictly CLEAR WS_EX_APPWINDOW
            let current_ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
            let target_ex = (current_ex & !(WS_EX_APPWINDOW.0 as i32))
                | (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0 | WS_EX_LAYERED.0) as i32;
            SetWindowLongW(hwnd, GWL_EXSTYLE, target_ex);

            // Exclude from captures using WDA_EXCLUDEFROMCAPTURE (0x00000011)
            let _ = SetWindowDisplayAffinity(hwnd, WINDOW_DISPLAY_AFFINITY(0x00000011));

            // Disable DWM non-client rendering policy to ensure zero shadow
            let disable_nc: DWMNCRENDERINGPOLICY = DWMNCRP_DISABLED;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_NCRENDERING_POLICY,
                &disable_nc as *const _ as *const _,
                std::mem::size_of::<DWMNCRENDERINGPOLICY>() as u32,
            );
        }
    }
}

pub fn run() {
    let is_diagnostic = std::env::args().any(|a| a == "--diagnostic");
    if is_diagnostic {
        println!("============================================================");
        println!("RUBY ASSISTANT - M1 WISPR FLOW DICTATION DIAGNOSTIC MODE");
        println!("============================================================");
        println!("* Low-Level Keyboard Hook: Ctrl+Win hold-to-talk (>=250ms)");
        println!("* Double-Tap: Hands-free toggle (<350ms interval)");
        println!("* Start Menu Suppression: VK_NONAME (0xFC) SendInput active");
        println!("* Cancel: Esc | Abort: Any 3rd key | Kill Switch: Ctrl+Alt+Shift+R");
        println!("* Audio: cpal 16kHz mono -> Energy VAD -> gemini-3.8-flash (thinkingLevel: low)");
        println!("* Insertion: SendInput Unicode (<64 chars) / Clipboard paste (>=64 chars)");
        println!("============================================================");
    }

    let bot_token = ruby_remote::get_telegram_bot_token().unwrap_or_default();
    let remote_adapter = Arc::new(TelegramAdapter::new(bot_token));

    let builder = tauri::Builder::default().manage(AppState {
        orb_state: Mutex::new(OrbVisualState::default()),
        seq_counter: AtomicU64::new(1),
        click_through: AtomicBool::new(false),
        remote_adapter: Arc::clone(&remote_adapter),
    });

    #[cfg(feature = "test-capture")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::get_orb_state,
        commands::set_orb_state,
        commands::toggle_orb_click_through,
        commands::get_remote_status,
        commands::disconnect_remote,
        commands::save_orb_snapshot
    ]);

    #[cfg(not(feature = "test-capture"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::get_orb_state,
        commands::set_orb_state,
        commands::toggle_orb_click_through,
        commands::get_remote_status,
        commands::disconnect_remote
    ]);

    let remote_adapter_bg = Arc::clone(&remote_adapter);
    let remote_cfg = ruby_remote::load_remote_config();
    if remote_cfg.enabled && ruby_remote::get_telegram_bot_token().is_some() {
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(_) => return,
            };
            rt.block_on(async {
                let _ = remote_adapter_bg
                    .start_polling(Arc::new(NoopCommandHandler))
                    .await;
            });
        });
    }

    builder
        .setup(|app| {
            if let Some(orb_win) = app.get_webview_window("orb") {
                configure_orb_window_win32(&orb_win.as_ref().window());
            }

            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                let kill_switch = Arc::new(AtomicBool::new(false));
                let desktop_controller = Box::new(WindowsDesktopController::new(Arc::clone(&kill_switch)));
                let audio_source = Box::new(CpalAudioCapture::new());
                let gemini_client = Box::new(GeminiDictationClient::default());

                let app_handle_cb = app_handle.clone();
                let dictation_engine = Arc::new(SystemDictationEngine::new(
                    audio_source,
                    gemini_client,
                    desktop_controller,
                    Some(Arc::new(move |dict_state| {
                        let op_str = dict_state.to_orb_operational_label();
                        let op = match op_str {
                            "Listening" => OperationalState::Listening,
                            "Thinking" => OperationalState::Thinking,
                            "Speaking" => OperationalState::Speaking,
                            "Error" => OperationalState::Error,
                            _ => OperationalState::Idle,
                        };
                        let state = app_handle_cb.state::<AppState>();
                        let seq = state.seq_counter.fetch_add(1, Ordering::SeqCst);
                        let mut orb = state.orb_state.lock().unwrap();
                        orb.operational = op;
                        let payload = SequencedOrbEvent { seq, state: *orb };
                        let _ = app_handle_cb.emit("orb_state_changed", payload);
                    })),
                ));

                if let Ok(hook_handle) = KeyboardHookHandle::start() {
                    let rt = match tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        Ok(r) => r,
                        Err(_) => return,
                    };

                    rt.block_on(async {
                        while let Ok(action) = hook_handle.receiver().recv() {
                            match action {
                                HotKeyAction::PreRollStarted => {
                                    let _ = dictation_engine.start_pre_roll().await;
                                }
                                HotKeyAction::PreRollDiscarded => {
                                    let _ = dictation_engine.discard_pre_roll().await;
                                }
                                HotKeyAction::HoldStarted => {
                                    let _ = dictation_engine.commit_pre_roll().await;
                                }
                                HotKeyAction::HoldReleased { .. } => {
                                    let target_proc = ruby_platform::elevation_check::get_foreground_process_name();
                                    let res = dictation_engine.finish_and_insert(target_proc).await;
                                    if res.is_ok() {
                                        if let Some(backend) = dictation_engine.last_served_backend() {
                                            if backend.starts_with("freellmapi") {
                                                let _ = app_handle.emit("console_notice", "served via FreeLLMAPI");
                                                let _ = app_handle.emit("toast_notification", "served via FreeLLMAPI");
                                            }
                                        }
                                    } else if let Err(ruby_dictation::DictationError::Insertion(ref s)) = res {
                                        if s.contains("ElevatedTargetBlocked") {
                                            let _ = app_handle.emit("toast_notification", "Target window is elevated (Administrator). Run Ruby as Administrator to dictate into elevated prompts.");
                                        }
                                    }
                                }
                                HotKeyAction::DoubleTapToggled { active } => {
                                    if active {
                                        let _ = dictation_engine.start_recording().await;
                                    } else {
                                        let target_proc = ruby_platform::elevation_check::get_foreground_process_name();
                                        let res = dictation_engine.finish_and_insert(target_proc).await;
                                        if res.is_ok() {
                                            if let Some(backend) = dictation_engine.last_served_backend() {
                                                if backend.starts_with("freellmapi") {
                                                    let _ = app_handle.emit("console_notice", "served via FreeLLMAPI");
                                                    let _ = app_handle.emit("toast_notification", "served via FreeLLMAPI");
                                                }
                                            }
                                        } else if let Err(ruby_dictation::DictationError::Insertion(ref s)) = res {
                                            if s.contains("ElevatedTargetBlocked") {
                                                let _ = app_handle.emit("toast_notification", "Target window is elevated (Administrator). Run Ruby as Administrator to dictate into elevated prompts.");
                                            }
                                        }
                                    }
                                }
                                HotKeyAction::Cancelled => {
                                    dictation_engine.cancel().await;
                                }
                                HotKeyAction::AbortedThirdKey { .. } => {
                                    dictation_engine.cancel().await;
                                }
                                HotKeyAction::KillSwitchTriggered => {
                                    kill_switch.store(true, Ordering::SeqCst);
                                    dictation_engine.cancel().await;
                                    std::process::exit(0);
                                }
                            }
                        }
                    });
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running ruby application");
}
