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

use ruby_core::chat::ChatSession;
use ruby_core::tts::{
    EdgeTtsEngine, GeminiTtsEngine, TtsFallbackChain, TtsPlaybackController, WindowsSpeechEngine,
};
use ruby_core::{OperationalState, OrbVisualState};
use ruby_dictation::SystemDictationEngine;
use ruby_gemini::{ChatClient, GeminiChatClient, GeminiDictationClient};
use ruby_platform::{HotKeyAction, KeyboardHookHandle, WindowsDesktopController};
use ruby_remote::{ChannelAdapter, TelegramAdapter};
use ruby_voice::CpalAudioCapture;

pub mod commands;

pub struct AppState {
    pub orb_state: Arc<Mutex<OrbVisualState>>,
    pub seq_counter: Arc<AtomicU64>,
    pub click_through: AtomicBool,
    pub remote_adapter: Arc<TelegramAdapter>,
    pub chat_client: Arc<dyn ChatClient>,
    pub chat_session: Arc<tokio::sync::Mutex<ChatSession>>,
    pub tts_chain: Arc<TtsFallbackChain>,
    pub tts_playback: Arc<TtsPlaybackController>,
    pub tool_manager: Arc<ruby_mcp::PolicyGatedToolManager>,
    pub memory_store: Arc<dyn ruby_memory::MemoryStore>,
    pub computer_use_kill_switch: Arc<ruby_actions::computer_use::KillSwitchToken>,
    pub computer_use_audit: Arc<Mutex<Vec<ruby_actions::computer_use::StepRecord>>>,
    pub computer_use_active_goal: Arc<Mutex<Option<String>>>,
    pub voice_live_manager: Arc<ruby_core::voice_live::VoiceLiveManager>,
    pub bg_supervisor: Arc<ruby_core::tasks::BackgroundTaskSupervisor>,
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

    let unified_confirmations = Arc::new(ruby_policy::UnifiedConfirmationManager::new());
    let remote_confirmation_mgr =
        ruby_remote::ConfirmationManager::from_unified(Arc::clone(&unified_confirmations));

    let bot_token = ruby_remote::get_telegram_bot_token().unwrap_or_default();
    let remote_adapter = Arc::new(TelegramAdapter::with_confirmation_manager(
        bot_token,
        remote_confirmation_mgr,
    ));

    let chat_client: Arc<dyn ChatClient> = Arc::new(GeminiChatClient::default());
    let chat_session = Arc::new(tokio::sync::Mutex::new(ChatSession::new()));
    let tts_chain = Arc::new(TtsFallbackChain::new(vec![
        Arc::new(EdgeTtsEngine::default()),
        Arc::new(GeminiTtsEngine),
        Arc::new(WindowsSpeechEngine),
    ]));
    let tts_playback = Arc::new(TtsPlaybackController::new());

    let orb_state_ref = Arc::new(Mutex::new(OrbVisualState::default()));
    let seq_counter_ref = Arc::new(AtomicU64::new(1));

    let tool_policy = Arc::new(
        ruby_policy::DefaultToolSecurityPolicy::with_allowed_domains(vec![
            "github.com".into(),
            "*.github.com".into(),
            "crates.io".into(),
            "docs.rs".into(),
        ]),
    );
    let sidecar_config = ruby_mcp::McpRegistry::default_playwright_config();
    let sidecar = Arc::new(ruby_mcp::PlaywrightSidecarManager::new(sidecar_config));
    let tool_manager = Arc::new(ruby_mcp::PolicyGatedToolManager::with_confirmations(
        tool_policy,
        sidecar,
        Arc::clone(&unified_confirmations),
    ));

    let db_path = ruby_memory::get_default_db_path();
    let memory_store: Arc<dyn ruby_memory::MemoryStore> =
        match ruby_memory::SqliteMemoryStore::open(&db_path) {
            Ok(s) => Arc::new(s),
            Err(_) => Arc::new(ruby_memory::SqliteMemoryStore::open_in_memory().unwrap()),
        };

    let computer_use_kill_switch = Arc::new(ruby_actions::computer_use::KillSwitchToken::new());
    let computer_use_audit = Arc::new(Mutex::new(Vec::new()));
    let computer_use_active_goal = Arc::new(Mutex::new(None));

    // M5: Voice Live manager — uses NullAudioOutput and MockToolDispatcher at startup;
    // real audio output and policy-gated dispatcher are wired in the Tauri setup below.
    let voice_live_manager = Arc::new(ruby_core::voice_live::VoiceLiveManager::new(
        Arc::new(ruby_core::voice_live::NullAudioOutput),
        Arc::new(ruby_core::voice_live::MockToolDispatcher::default()),
        None, // orb callback wired after app handle available
    ));

    let ruby_paths = ruby_config::RubyPaths::resolve();
    let jobs_db_path = ruby_paths.workspace_dir.join("jobs.db");
    let job_store = Arc::new(
        ruby_core::tasks::SqliteJobStore::new(&jobs_db_path)
            .unwrap_or_else(|_| ruby_core::tasks::SqliteJobStore::new_in_memory().unwrap()),
    );
    let tool_backend = Arc::new(ruby_core::tasks::LocalToolBackend::new(
        ruby_paths.workspace_dir,
        false,
    ));
    let bg_supervisor = Arc::new(ruby_core::tasks::BackgroundTaskSupervisor::new(
        job_store,
        tool_backend,
        300,
    ));
    remote_adapter.set_background_supervisor(Arc::clone(&bg_supervisor));

    let builder = tauri::Builder::default().manage(AppState {
        orb_state: Arc::clone(&orb_state_ref),
        seq_counter: Arc::clone(&seq_counter_ref),
        click_through: AtomicBool::new(false),
        remote_adapter: Arc::clone(&remote_adapter),
        chat_client,
        chat_session,
        tts_chain,
        tts_playback,
        tool_manager,
        memory_store,
        computer_use_kill_switch,
        computer_use_audit,
        computer_use_active_goal,
        voice_live_manager,
        bg_supervisor,
    });

    #[cfg(feature = "test-capture")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::get_orb_state,
        commands::set_orb_state,
        commands::toggle_orb_click_through,
        commands::get_remote_status,
        commands::disconnect_remote,
        commands::send_chat_message,
        commands::cancel_speech,
        commands::save_orb_snapshot,
        commands::execute_tool,
        commands::confirm_tool_execution,
        commands::add_memory,
        commands::search_memories,
        commands::delete_memory,
        commands::wipe_all_memories,
        commands::start_computer_use_task,
        commands::stop_computer_use_task,
        commands::get_computer_use_status,
        commands::start_voice_live,
        commands::stop_voice_live,
        commands::get_voice_live_status,
        commands::get_voice_live_transcript,
        commands::push_voice_live_audio,
        commands::submit_background_job,
        commands::list_background_jobs,
        commands::get_background_job,
        commands::cancel_background_job,
        commands::get_background_job_logs
    ]);

    #[cfg(not(feature = "test-capture"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::get_orb_state,
        commands::set_orb_state,
        commands::toggle_orb_click_through,
        commands::get_remote_status,
        commands::disconnect_remote,
        commands::send_chat_message,
        commands::cancel_speech,
        commands::execute_tool,
        commands::confirm_tool_execution,
        commands::add_memory,
        commands::search_memories,
        commands::delete_memory,
        commands::wipe_all_memories,
        commands::start_computer_use_task,
        commands::stop_computer_use_task,
        commands::get_computer_use_status,
        commands::start_voice_live,
        commands::stop_voice_live,
        commands::get_voice_live_status,
        commands::get_voice_live_transcript,
        commands::push_voice_live_audio,
        commands::submit_background_job,
        commands::list_background_jobs,
        commands::get_background_job,
        commands::cancel_background_job,
        commands::get_background_job_logs
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
                let remote_cfg = ruby_remote::key_storage::load_remote_config();
                let policy = Arc::new(ruby_policy::DefaultSecurityPolicy::new(
                    remote_cfg.allowed_roots,
                ));
                let executor = Arc::new(ruby_actions::AllowlistedActionExecutor);
                let confirmation_mgr = remote_adapter_bg.confirmation_manager();
                let task_mgr = Arc::new(remote_adapter_bg.task_manager());
                let handler = Arc::new(ruby_remote::PolicyGatedCommandHandler::with_task_manager(
                    policy,
                    executor,
                    confirmation_mgr,
                    task_mgr,
                ));
                let _ = remote_adapter_bg.start_polling(handler).await;
            });
        });
    }

    builder
        .setup(|app| {
            if let Some(orb_win) = app.get_webview_window("orb") {
                configure_orb_window_win32(&orb_win.as_ref().window());
            }
            if let Some(console_win) = app.get_webview_window("console").or_else(|| app.get_webview_window("main")) {
                if let Ok(hwnd_val) = console_win.as_ref().window().hwnd() {
                    let hwnd = HWND(hwnd_val.0 as *mut _);
                    unsafe {
                        let _ = SetWindowDisplayAffinity(hwnd, WINDOW_DISPLAY_AFFINITY(0x00000011));
                    }
                }
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
                        let seq = seq_counter_ref.fetch_add(1, Ordering::SeqCst) + 1;
                        let mut orb = orb_state_ref.lock().unwrap();
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
                                    let target_proc =
                                        ruby_platform::elevation_check::get_foreground_process_name();
                                    let res = dictation_engine.finish_and_insert(target_proc).await;
                                    if res.is_ok() {
                                        if let Some(backend) = dictation_engine.last_served_backend() {
                                            if backend.starts_with("freellmapi") {
                                                let _ = app_handle
                                                    .emit("console_notice", "served via FreeLLMAPI");
                                                let _ = app_handle.emit(
                                                    "toast_notification",
                                                    "served via FreeLLMAPI",
                                                );
                                            }
                                        }
                                    } else if let Err(ruby_dictation::DictationError::Insertion(
                                        ref s,
                                    )) = res
                                    {
                                        if s.contains("ElevatedTargetBlocked") {
                                            let _ = app_handle.emit("toast_notification", "Target window is elevated (Administrator). Run Ruby as Administrator to dictate into elevated prompts.");
                                        }
                                    }
                                }
                                HotKeyAction::DoubleTapToggled { active } => {
                                    if active {
                                        let _ = dictation_engine.start_recording().await;
                                    } else {
                                        let target_proc =
                                            ruby_platform::elevation_check::get_foreground_process_name();
                                        let res = dictation_engine.finish_and_insert(target_proc).await;
                                        if res.is_ok() {
                                            if let Some(backend) = dictation_engine.last_served_backend() {
                                                if backend.starts_with("freellmapi") {
                                                    let _ = app_handle
                                                        .emit("console_notice", "served via FreeLLMAPI");
                                                    let _ = app_handle.emit(
                                                        "toast_notification",
                                                        "served via FreeLLMAPI",
                                                    );
                                                }
                                            }
                                        } else if let Err(ruby_dictation::DictationError::Insertion(
                                            ref s,
                                        )) = res
                                        {
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
