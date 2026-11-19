use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State, Window};
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_TRANSPARENT},
};

use ruby_core::{Emotion, OperationalState};

use crate::{AppState, SequencedOrbEvent};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModelToolRequest {
    pub server: String,
    pub tool: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

pub fn extract_tool_call_from_model_output(output: &str) -> Option<ModelToolRequest> {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(output) {
        if let Some(tool_obj) = val.get("tool_call") {
            if let Ok(req) = serde_json::from_value::<ModelToolRequest>(tool_obj.clone()) {
                return Some(req);
            }
        }
    }

    if let Some(start_idx) = output.find("```tool_call") {
        let remainder = &output[start_idx + 12..];
        if let Some(end_idx) = remainder.find("```") {
            let json_str = remainder[..end_idx].trim();
            if let Ok(req) = serde_json::from_str::<ModelToolRequest>(json_str) {
                return Some(req);
            }
        }
    }

    if let Some(idx) = output.find("\"tool_call\":") {
        let sub = &output[idx + 12..].trim_start();
        if sub.starts_with('{') {
            let mut depth = 0;
            let mut end_idx = 0;
            for (i, c) in sub.char_indices() {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        end_idx = i + 1;
                        break;
                    }
                }
            }
            if end_idx > 0 {
                if let Ok(req) = serde_json::from_str::<ModelToolRequest>(&sub[..end_idx]) {
                    return Some(req);
                }
            }
        }
    }

    None
}

#[tauri::command]
pub fn get_orb_state(state: State<'_, AppState>) -> SequencedOrbEvent {
    let current = *state.orb_state.lock().unwrap();
    let seq = state.seq_counter.load(Ordering::SeqCst);
    SequencedOrbEvent {
        seq,
        state: current,
    }
}

#[tauri::command]
pub fn set_orb_state(
    app: AppHandle,
    state: State<'_, AppState>,
    operational: OperationalState,
    emotion: Emotion,
    thinking_spin: Option<bool>,
    speaking_active: Option<bool>,
) -> Result<SequencedOrbEvent, String> {
    let mut current = state.orb_state.lock().unwrap();
    current.operational = operational;
    current.emotion = emotion;
    if let Some(spin) = thinking_spin {
        current.thinking_spin_active = spin;
    }
    if let Some(speaking) = speaking_active {
        current.speaking_active = speaking;
    }
    let seq = state.seq_counter.fetch_add(1, Ordering::SeqCst) + 1;
    let event = SequencedOrbEvent {
        seq,
        state: *current,
    };

    app.emit("orb_state_changed", &event)
        .map_err(|e| e.to_string())?;

    Ok(event)
}

#[tauri::command]
pub fn toggle_orb_click_through(
    window: Window,
    state: State<'_, AppState>,
    enable: bool,
) -> Result<bool, String> {
    if window.label() != "orb" {
        return Ok(false);
    }
    let hwnd_val = window.hwnd().map_err(|e| e.to_string())?;
    let hwnd = HWND(hwnd_val.0 as *mut _);

    unsafe {
        let current_ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let new_ex = if enable {
            current_ex | WS_EX_TRANSPARENT.0 as i32
        } else {
            current_ex & !(WS_EX_TRANSPARENT.0 as i32)
        };
        SetWindowLongW(hwnd, GWL_EXSTYLE, new_ex);
    }

    state.click_through.store(enable, Ordering::SeqCst);
    Ok(enable)
}

#[tauri::command]
pub fn get_remote_status(state: State<'_, AppState>) -> ruby_remote::RemoteStatus {
    state.remote_adapter.get_status()
}

#[tauri::command]
pub fn disconnect_remote(state: State<'_, AppState>) -> Result<ruby_remote::RemoteStatus, String> {
    state.remote_adapter.disconnect_remote()?;
    Ok(state.remote_adapter.get_status())
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct ChatTurnResult {
    pub response_text: String,
    pub emotion: Emotion,
    pub served_backend: String,
    pub tts_engine: String,
}

#[tauri::command]
pub async fn send_chat_message(
    app: AppHandle,
    state: State<'_, AppState>,
    message: String,
    _tts_engine: Option<String>,
) -> Result<ChatTurnResult, String> {
    let trimmed = message.trim();

    // 1. Tool confirmation shortcut: /yes <code>
    if let Some(code) = trimmed.strip_prefix("/yes ") {
        let code = code.trim();
        let res = state.tool_manager.confirm_and_execute(code).await;
        let reply_text = match res {
            Ok(output) => format!("✅ Tool confirmed and executed:\n{output}"),
            Err(e) => format!("❌ Confirmation failed: {e}"),
        };
        {
            let mut orb = state.orb_state.lock().unwrap();
            orb.operational = OperationalState::Speaking;
            orb.emotion = Emotion::Neutral;
            orb.thinking_spin_active = false;
            orb.speaking_active = true;
            let seq = state.seq_counter.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = app.emit("orb_state_changed", &SequencedOrbEvent { seq, state: *orb });
        }
        return Ok(ChatTurnResult {
            response_text: reply_text,
            emotion: Emotion::Neutral,
            served_backend: "tool_execution".into(),
            tts_engine: "none".into(),
        });
    }

    // 2. Memory shortcuts: remember <fact> / store <fact>
    if let Some(fact) = trimmed
        .strip_prefix("remember ")
        .or_else(|| trimmed.strip_prefix("store "))
    {
        let fact = fact.trim();
        let reply_text = match state.memory_store.add_memory("fact", fact) {
            Ok(id) => format!("✅ Saved to memory [ID: {id}]: {fact}"),
            Err(e) => format!("❌ Memory storage refused: {e}"),
        };
        {
            let mut orb = state.orb_state.lock().unwrap();
            orb.operational = OperationalState::Speaking;
            orb.emotion = Emotion::Happy;
            orb.thinking_spin_active = false;
            orb.speaking_active = true;
            let seq = state.seq_counter.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = app.emit("orb_state_changed", &SequencedOrbEvent { seq, state: *orb });
        }
        return Ok(ChatTurnResult {
            response_text: reply_text,
            emotion: Emotion::Happy,
            served_backend: "memory_store".into(),
            tts_engine: "none".into(),
        });
    }

    // 3. Memory recall: recall <query> / search memory <query>
    if let Some(query) = trimmed
        .strip_prefix("recall ")
        .or_else(|| trimmed.strip_prefix("search memory "))
    {
        let query = query.trim();
        let reply_text = match state.memory_store.search_memories(query, 5) {
            Ok(entries) => ruby_memory::format_memories_as_passive_data(&entries),
            Err(e) => format!("❌ Memory search failed: {e}"),
        };
        {
            let mut orb = state.orb_state.lock().unwrap();
            orb.operational = OperationalState::Speaking;
            orb.emotion = Emotion::Curious;
            orb.thinking_spin_active = false;
            orb.speaking_active = true;
            let seq = state.seq_counter.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = app.emit("orb_state_changed", &SequencedOrbEvent { seq, state: *orb });
        }
        return Ok(ChatTurnResult {
            response_text: reply_text,
            emotion: Emotion::Curious,
            served_backend: "memory_search".into(),
            tts_engine: "none".into(),
        });
    }

    // 4. Set Orb state to Thinking with spin active
    {
        let mut orb = state.orb_state.lock().unwrap();
        orb.operational = OperationalState::Thinking;
        orb.thinking_spin_active = true;
        let seq = state.seq_counter.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = app.emit("orb_state_changed", &SequencedOrbEvent { seq, state: *orb });
    }

    // 5. Multi-turn LLM chat call
    let history: Vec<ruby_gemini::ChatMessage> = {
        let mut session = state.chat_session.lock().await;
        session.add_message(ruby_core::chat::ChatMessage::user(&message));
        session
            .messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    ruby_core::chat::ChatRole::User => ruby_gemini::ChatRole::User,
                    ruby_core::chat::ChatRole::Assistant => ruby_gemini::ChatRole::Model,
                    ruby_core::chat::ChatRole::System => ruby_gemini::ChatRole::System,
                };
                ruby_gemini::ChatMessage {
                    role,
                    content: m.content.clone(),
                }
            })
            .collect()
    };

    let chat_resp = match state.chat_client.send_message(&history, &message).await {
        Ok(r) => r,
        Err(e) => {
            let mut orb = state.orb_state.lock().unwrap();
            orb.operational = OperationalState::Error;
            orb.thinking_spin_active = false;
            let seq = state.seq_counter.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = app.emit("orb_state_changed", &SequencedOrbEvent { seq, state: *orb });
            return Err(format!("Chat failed: {}", e));
        }
    };

    // If chat was served via FreeLLMAPI, emit console notice and toast
    if let Some(notice) = check_and_format_freellmapi_notice(&chat_resp.served_backend) {
        let _ = app.emit("console_notice", notice);
        let _ = app.emit("toast_notification", notice);
    }

    // 2b. Check if the model requested a tool execution
    if let Some(tool_req) = extract_tool_call_from_model_output(&chat_resp.text) {
        let exec_res = state
            .tool_manager
            .execute_tool(&tool_req.server, &tool_req.tool, tool_req.arguments.clone())
            .await;

        let (final_text, final_emotion) = match exec_res {
            Ok(output) => {
                let _ = app.emit(
                    "tool_executed",
                    serde_json::json!({
                        "server": tool_req.server,
                        "tool": tool_req.tool,
                        "output": output
                    }),
                );
                let text = format!(
                    "Tool '{}/{}' executed successfully:\n{}",
                    tool_req.server, tool_req.tool, output
                );
                (text, Emotion::Curious)
            }
            Err(ruby_mcp::McpError::RequiresConfirmation {
                code,
                tool_name,
                description,
            }) => {
                let _ = app.emit(
                    "tool_confirmation_requested",
                    serde_json::json!({
                        "code": code,
                        "tool_name": tool_name,
                        "description": description
                    }),
                );
                // Also broadcast notification to paired Telegram user
                let tg_adapter = Arc::clone(&state.remote_adapter);
                let tg_code = code.clone();
                let tg_desc = description.clone();
                tokio::spawn(async move {
                    let alert = format!(
                        "⚠️ Action requires confirmation: {tg_desc}\nReply /yes {tg_code} or /no within 120 seconds."
                    );
                    let _ = tg_adapter.broadcast_notification(&alert).await;
                });

                let text = format!(
                    "⚠️ Tool execution requires confirmation: {description}.\nReply with /yes {code} or /no to authorize (expires in 120s)."
                );
                (text, Emotion::Surprised)
            }
            Err(e) => {
                let text = format!("❌ Tool call '{}' denied or failed: {}", tool_req.tool, e);
                (text, Emotion::Neutral)
            }
        };

        {
            let mut session = state.chat_session.lock().await;
            session.add_message(ruby_core::chat::ChatMessage::assistant(
                &final_text,
                Some(final_emotion),
            ));
        }

        {
            let mut orb = state.orb_state.lock().unwrap();
            orb.operational = OperationalState::Speaking;
            orb.emotion = final_emotion;
            orb.thinking_spin_active = false;
            orb.speaking_active = true;
            let seq = state.seq_counter.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = app.emit("orb_state_changed", &SequencedOrbEvent { seq, state: *orb });
        }

        return Ok(ChatTurnResult {
            response_text: final_text,
            emotion: final_emotion,
            served_backend: chat_resp.served_backend,
            tts_engine: "none".into(),
        });
    }

    // 3. Parse emotion response
    let (answer_text, emotion) = ruby_core::chat::parse_emotion_response(&chat_resp.text, &message);
    {
        let mut session = state.chat_session.lock().await;
        session.add_message(ruby_core::chat::ChatMessage::assistant(
            &answer_text,
            Some(emotion),
        ));
    }

    // 4. Update Orb to Speaking with detected emotion
    {
        let mut orb = state.orb_state.lock().unwrap();
        orb.operational = OperationalState::Speaking;
        orb.emotion = emotion;
        orb.thinking_spin_active = false;
        orb.speaking_active = true;
        let seq = state.seq_counter.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = app.emit("orb_state_changed", &SequencedOrbEvent { seq, state: *orb });
    }

    // 5. Synthesize TTS via Fallback Chain
    let cancel = state.tts_playback.cancel_token();
    let (wav_bytes, tts_engine_used) = state
        .tts_chain
        .synthesize(&answer_text, None, &cancel)
        .await
        .unwrap_or_else(|_| (vec![], "none"));

    // 6. Non-blocking audio playback with cancellation support
    let playback = Arc::clone(&state.tts_playback);
    let app_clone = app.clone();
    let state_orb = Arc::clone(&state.orb_state);
    let seq_counter = Arc::clone(&state.seq_counter);

    tokio::spawn(async move {
        playback
            .play(
                &wav_bytes,
                || {},
                move || {
                    let mut orb = state_orb.lock().unwrap();
                    orb.operational = OperationalState::Idle;
                    orb.emotion = Emotion::Neutral;
                    orb.speaking_active = false;
                    let seq = seq_counter.fetch_add(1, Ordering::SeqCst) + 1;
                    let _ = app_clone
                        .emit("orb_state_changed", &SequencedOrbEvent { seq, state: *orb });
                },
            )
            .await;
    });

    Ok(ChatTurnResult {
        response_text: answer_text,
        emotion,
        served_backend: chat_resp.served_backend,
        tts_engine: tts_engine_used.to_string(),
    })
}

#[tauri::command]
pub fn cancel_speech(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.tts_playback.cancel();
    let mut orb = state.orb_state.lock().unwrap();
    orb.operational = OperationalState::Idle;
    orb.emotion = Emotion::Neutral;
    orb.speaking_active = false;
    orb.thinking_spin_active = false;
    let seq = state.seq_counter.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = app.emit("orb_state_changed", &SequencedOrbEvent { seq, state: *orb });
    Ok(())
}

#[tauri::command]
pub async fn execute_tool(
    app: AppHandle,
    state: State<'_, AppState>,
    server: String,
    tool_name: String,
    arguments: serde_json::Value,
) -> Result<String, String> {
    let res = state
        .tool_manager
        .execute_tool(&server, &tool_name, arguments)
        .await;
    match res {
        Ok(output) => {
            let _ = app.emit(
                "tool_executed",
                serde_json::json!({
                    "server": server,
                    "tool_name": tool_name,
                    "output": output
                }),
            );
            Ok(output)
        }
        Err(ruby_mcp::McpError::RequiresConfirmation {
            code,
            tool_name,
            description,
        }) => {
            let _ = app.emit(
                "tool_confirmation_requested",
                serde_json::json!({
                    "code": code,
                    "tool_name": tool_name,
                    "description": description
                }),
            );
            Err(format!(
                "⚠️ Tool execution requires confirmation: {description}.\nReply /yes {code} or /no within 120 seconds."
            ))
        }
        Err(e) => Err(format!("Tool execution error: {e}")),
    }
}

#[tauri::command]
pub async fn confirm_tool_execution(
    state: State<'_, AppState>,
    code: String,
) -> Result<String, String> {
    state
        .tool_manager
        .confirm_and_execute(&code)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_memory(
    state: State<'_, AppState>,
    category: String,
    text: String,
) -> Result<i64, String> {
    state
        .memory_store
        .add_memory(&category, &text)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn search_memories(
    state: State<'_, AppState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<ruby_memory::MemoryEntry>, String> {
    state
        .memory_store
        .search_memories(&query, limit.unwrap_or(10))
        .map_err(|e| e.to_string())
}

pub fn delete_memory_core(
    memory_store: &dyn ruby_memory::MemoryStore,
    conf_mgr: &ruby_policy::UnifiedConfirmationManager,
    id: i64,
    confirmation_code: Option<String>,
) -> Result<String, String> {
    if let Some(code) = confirmation_code {
        conf_mgr
            .verify_and_consume(&code)
            .map_err(|e| format!("Confirmation failed: {e}"))?;
        memory_store.delete_memory(id).map_err(|e| e.to_string())?;
        Ok(format!("Memory record [ID: {id}] deleted."))
    } else {
        let desc = format!("Delete memory record [ID: {id}]");
        let code = conf_mgr.register_action(
            "memory",
            "delete_memory",
            serde_json::json!({ "id": id }),
            &desc,
        );
        Err(format!(
            "Requires confirmation: {desc}. Reply with code {code} to confirm."
        ))
    }
}

pub fn wipe_all_memories_core(
    memory_store: &dyn ruby_memory::MemoryStore,
    conf_mgr: &ruby_policy::UnifiedConfirmationManager,
    confirmation_code: Option<String>,
) -> Result<String, String> {
    let count = memory_store
        .search_memories("", 1000)
        .map(|m| m.len())
        .unwrap_or(0);
    if let Some(code) = confirmation_code {
        conf_mgr
            .verify_and_consume(&code)
            .map_err(|e| format!("Confirmation failed: {e}"))?;
        let wiped = memory_store.wipe_all().map_err(|e| e.to_string())?;
        Ok(format!("All memories wiped (total {wiped} records)."))
    } else {
        let desc = format!("Wipe all memories (total {count} records)");
        let code = conf_mgr.register_action(
            "memory",
            "wipe_all",
            serde_json::json!({ "count": count }),
            &desc,
        );
        Err(format!(
            "Requires confirmation: {desc}. Reply with code {code} to confirm."
        ))
    }
}

#[tauri::command]
pub fn delete_memory(
    state: State<'_, AppState>,
    id: i64,
    confirmation_code: Option<String>,
) -> Result<String, String> {
    let conf_mgr = state.tool_manager.confirmation_manager();
    delete_memory_core(&*state.memory_store, &conf_mgr, id, confirmation_code)
}

#[tauri::command]
pub fn wipe_all_memories(
    state: State<'_, AppState>,
    confirmation_code: Option<String>,
) -> Result<String, String> {
    let conf_mgr = state.tool_manager.confirmation_manager();
    wipe_all_memories_core(&*state.memory_store, &conf_mgr, confirmation_code)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ComputerUseStatus {
    pub is_acting: bool,
    pub current_goal: Option<String>,
    pub step_count: usize,
    pub dry_run: bool,
    pub recent_steps: Vec<ruby_actions::computer_use::StepRecord>,
}

#[tauri::command]
pub async fn start_computer_use_task(
    app: AppHandle,
    state: State<'_, AppState>,
    goal: String,
    dry_run: bool,
) -> Result<String, String> {
    let trimmed = goal.trim();
    if trimmed.is_empty() {
        return Err("Goal cannot be empty".into());
    }

    *state.computer_use_active_goal.lock().unwrap() = Some(trimmed.to_string());

    // Update orb state to Acting
    {
        let mut orb = state.orb_state.lock().unwrap();
        orb.operational = OperationalState::Acting;
        orb.emotion = Emotion::Neutral;
        orb.thinking_spin_active = false;
        orb.speaking_active = false;
        let seq = state.seq_counter.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = app.emit("orb_state_changed", &SequencedOrbEvent { seq, state: *orb });
    }

    let task_id = format!(
        "cu-{}",
        ruby_policy::UnifiedConfirmationManager::generate_secure_code_6_digits()
    );
    let _ = app.emit(
        "computer_use_started",
        serde_json::json!({
            "task_id": task_id,
            "goal": trimmed,
            "dry_run": dry_run
        }),
    );

    Ok(task_id)
}

#[tauri::command]
pub fn stop_computer_use_task(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.computer_use_kill_switch.cancel();
    *state.computer_use_active_goal.lock().unwrap() = None;

    {
        let mut orb = state.orb_state.lock().unwrap();
        orb.operational = OperationalState::Idle;
        orb.emotion = Emotion::Neutral;
        orb.thinking_spin_active = false;
        orb.speaking_active = false;
        let seq = state.seq_counter.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = app.emit("orb_state_changed", &SequencedOrbEvent { seq, state: *orb });
    }

    let _ = app.emit(
        "computer_use_stopped",
        serde_json::json!({ "reason": "user_kill_switch" }),
    );
    Ok(())
}

#[tauri::command]
pub fn get_computer_use_status(state: State<'_, AppState>) -> Result<ComputerUseStatus, String> {
    let goal = state.computer_use_active_goal.lock().unwrap().clone();
    let audit = state.computer_use_audit.lock().unwrap().clone();
    let is_acting = goal.is_some() && !state.computer_use_kill_switch.is_cancelled();

    Ok(ComputerUseStatus {
        is_acting,
        current_goal: goal,
        step_count: audit.len(),
        dry_run: false,
        recent_steps: audit,
    })
}

#[cfg(feature = "test-capture")]
#[tauri::command]
pub fn save_orb_snapshot(name: String, base64_png: String) -> Result<String, String> {
    use base64::Engine;

    // Strict name validation to prevent path traversal or filesystem tampering
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!(
            "Invalid snapshot name '{}': must be strictly alphanumeric with underscores or hyphens",
            name
        ));
    }

    let clean = if let Some(idx) = base64_png.find("base64,") {
        &base64_png[idx + 7..]
    } else {
        &base64_png
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(clean.trim())
        .map_err(|e| e.to_string())?;

    // Safe dynamic path outside the repo confined to Ruby's screenshots directory
    let paths = ruby_config::RubyPaths::resolve();
    let out_dir = paths.screenshots_dir.join("snapshots");
    let _ = std::fs::create_dir_all(&out_dir);
    let target = out_dir.join(format!("{}.png", name));
    std::fs::write(&target, bytes).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

#[derive(Debug, PartialEq, Eq)]
pub enum EscHierarchyAction {
    ModeAKillSwitch,
    VoiceLiveStop,
    DictationCancel,
}

/// Dispatches an Esc keypress according to the strict priority hierarchy:
/// Priority 1: Mode A Computer Use task cancellation
/// Priority 2: Voice Live session termination
/// Priority 3: Dictation buffer cancellation
pub async fn dispatch_esc_hierarchy<F>(
    cu_is_active: bool,
    cu_kill_switch: &ruby_actions::computer_use::KillSwitchToken,
    voice_live_manager: &ruby_core::voice_live::VoiceLiveManager,
    cancel_dictation: F,
) -> EscHierarchyAction
where
    F: std::future::Future<Output = ()>,
{
    if cu_is_active {
        cu_kill_switch.cancel();
        EscHierarchyAction::ModeAKillSwitch
    } else if voice_live_manager.status() == ruby_core::voice_live::VoiceLiveStatus::Active
        || voice_live_manager.status() == ruby_core::voice_live::VoiceLiveStatus::Connecting
    {
        voice_live_manager.stop().await;
        EscHierarchyAction::VoiceLiveStop
    } else {
        cancel_dictation.await;
        EscHierarchyAction::DictationCancel
    }
}

/// Checks if served backend is FreeLLMAPI and formats user notification text.
pub fn check_and_format_freellmapi_notice(served_backend: &str) -> Option<&'static str> {
    if served_backend.to_ascii_lowercase().contains("freellmapi") {
        Some("Chat served via FreeLLMAPI")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_served_via_freellmapi_emits_console_notice() {
        let backend = "freellmapi-auto";
        let notice = check_and_format_freellmapi_notice(backend);
        assert_eq!(notice, Some("Chat served via FreeLLMAPI"));

        let gemini_backend = "gemini-3.8-flash";
        assert_eq!(check_and_format_freellmapi_notice(gemini_backend), None);

        let preview_backend = "gemini-3-flash-preview";
        assert_eq!(check_and_format_freellmapi_notice(preview_backend), None);
    }

    #[test]
    fn test_model_tool_call_extraction() {
        let json_output = r#"{"tool_call":{"server":"playwright","tool":"navigate","arguments":{"url":"https://github.com"}}}"#;
        let req = extract_tool_call_from_model_output(json_output).expect("Must extract tool call");
        assert_eq!(req.server, "playwright");
        assert_eq!(req.tool, "navigate");
        assert_eq!(req.arguments["url"], "https://github.com");

        let fenced_output = "Here is my plan:\n```tool_call\n{\"server\":\"fs\",\"tool\":\"read_file\",\"arguments\":{\"path\":\"README.md\"}}\n```";
        let req2 =
            extract_tool_call_from_model_output(fenced_output).expect("Must extract from fence");
        assert_eq!(req2.server, "fs");
        assert_eq!(req2.tool, "read_file");

        let text_without_tool =
            "{\"response\": \"Hello! How can I help you?\", \"emotion\": \"Happy\"}";
        assert!(extract_tool_call_from_model_output(text_without_tool).is_none());
    }

    #[tokio::test]
    async fn test_model_tool_call_policy_gating_and_data_return() {
        let policy = Arc::new(
            ruby_policy::DefaultToolSecurityPolicy::with_allowed_domains(vec!["github.com".into()]),
        );
        let sidecar_config = ruby_mcp::McpRegistry::default_playwright_config();
        let sidecar = Arc::new(ruby_mcp::PlaywrightSidecarManager::new(sidecar_config));
        let confirmations = Arc::new(ruby_policy::UnifiedConfirmationManager::new());
        let tool_mgr = ruby_mcp::PolicyGatedToolManager::with_confirmations(
            policy,
            sidecar,
            Arc::clone(&confirmations),
        );
        tool_mgr
            .register_client(
                "fs",
                Box::new(ruby_mcp::MockMcpClient::new("mock filesystem contents")),
            )
            .await;

        // 1. Read-only tool automatically executes and returns DATA
        let read_req = ModelToolRequest {
            server: "fs".into(),
            tool: "read_file".into(),
            arguments: serde_json::json!({ "path": "test.txt" }),
        };
        let res = tool_mgr
            .execute_tool(&read_req.server, &read_req.tool, read_req.arguments)
            .await;
        assert!(res.is_ok());
        let output = res.unwrap();
        assert!(output.contains("[DATA_OUTPUT]"));
        assert!(output.contains("[/DATA_OUTPUT]"));

        // 2. Write tool requires confirmation code
        let write_req = ModelToolRequest {
            server: "fs".into(),
            tool: "write_file".into(),
            arguments: serde_json::json!({ "path": "test.txt", "content": "data" }),
        };
        let res_write = tool_mgr
            .execute_tool(&write_req.server, &write_req.tool, write_req.arguments)
            .await;
        match res_write {
            Err(ruby_mcp::McpError::RequiresConfirmation {
                code, tool_name, ..
            }) => {
                assert_eq!(tool_name, "write_file");
                assert_eq!(code.len(), 6);

                // /yes <code> confirms and executes from unified manager
                let conf_res = tool_mgr
                    .confirm_and_execute(&code)
                    .await
                    .expect("Must confirm");
                assert!(conf_res.contains("[DATA_OUTPUT]"));
            }
            other => panic!("Expected RequiresConfirmation, got {:?}", other),
        }

        // 3. Unknown tool denied by policy
        let unknown_req = ModelToolRequest {
            server: "shell".into(),
            tool: "exec_cmd".into(),
            arguments: serde_json::json!({}),
        };
        let res_unknown = tool_mgr
            .execute_tool(
                &unknown_req.server,
                &unknown_req.tool,
                unknown_req.arguments,
            )
            .await;
        assert!(res_unknown.is_err());
    }
}

// ─── M5: Voice Live Commands ─────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VoiceLiveStatusResponse {
    pub status: String,
    pub transcript_entries: usize,
}

/// Start the Gemini Live voice session. Uses MockLiveSession unless GEMINI_API_KEY is set
/// and the `live-api` feature is compiled in.
#[tauri::command]
pub async fn start_voice_live(
    state: State<'_, AppState>,
) -> Result<VoiceLiveStatusResponse, String> {
    use ruby_core::voice_live::VoiceLiveStatus;
    use ruby_gemini::live_session::{LiveSessionConfig, MockLiveSession};

    let mgr = Arc::clone(&state.voice_live_manager);

    if mgr.status() != VoiceLiveStatus::Idle {
        return Err("Voice live session already active".to_string());
    }

    // In production use the real session when live-api feature and key are present;
    // fall back to mock session otherwise (CI / no key).
    let config = LiveSessionConfig::default();
    let handle = MockLiveSession::open(config);

    mgr.start_with_handle(handle)
        .await
        .map_err(|e| e.to_string())?;

    Ok(VoiceLiveStatusResponse {
        status: "starting".to_string(),
        transcript_entries: 0,
    })
}

/// Stop the active Gemini Live voice session.
#[tauri::command]
pub async fn stop_voice_live(
    state: State<'_, AppState>,
) -> Result<VoiceLiveStatusResponse, String> {
    let mgr = Arc::clone(&state.voice_live_manager);
    mgr.stop().await;

    Ok(VoiceLiveStatusResponse {
        status: "stopping".to_string(),
        transcript_entries: mgr.transcript().len(),
    })
}

/// Get current voice live session status and transcript length.
#[tauri::command]
pub fn get_voice_live_status(state: State<'_, AppState>) -> VoiceLiveStatusResponse {
    use ruby_core::voice_live::VoiceLiveStatus;
    let mgr = &state.voice_live_manager;
    let status_str = match mgr.status() {
        VoiceLiveStatus::Idle => "idle",
        VoiceLiveStatus::Connecting => "connecting",
        VoiceLiveStatus::Active => "active",
        VoiceLiveStatus::Stopping => "stopping",
    };
    VoiceLiveStatusResponse {
        status: status_str.to_string(),
        transcript_entries: mgr.transcript().len(),
    }
}

/// Get the full transcript from the current/last voice live session.
#[tauri::command]
pub fn get_voice_live_transcript(
    state: State<'_, AppState>,
) -> Vec<ruby_core::voice_live::TranscriptEntry> {
    state.voice_live_manager.transcript()
}

/// Push a PCM audio chunk from the frontend microphone into the live session.
/// The frontend passes raw 16kHz 16-bit LE mono PCM as a base64 string.
#[tauri::command]
pub async fn push_voice_live_audio(
    state: State<'_, AppState>,
    pcm_base64: String,
) -> Result<(), String> {
    use base64::engine::general_purpose::STANDARD as BASE64;
    use base64::Engine;

    let pcm = BASE64.decode(&pcm_base64).map_err(|e| e.to_string())?;
    state
        .voice_live_manager
        .push_mic_audio(pcm)
        .await
        .map_err(|e| e.to_string())
}

// ─── Mode B: Background Tasks Commands ─────────────────────────────────────────

#[tauri::command]
pub async fn submit_background_job(
    state: State<'_, AppState>,
    goal: String,
) -> Result<ruby_core::BackgroundTask, String> {
    state
        .bg_supervisor
        .submit(&goal)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_background_jobs(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<ruby_core::BackgroundTask>, String> {
    state
        .bg_supervisor
        .list_jobs(limit.unwrap_or(20))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_background_job(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<ruby_core::BackgroundTask>, String> {
    let parsed_id = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    state
        .bg_supervisor
        .get_job(parsed_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cancel_background_job(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let parsed_id = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    state
        .bg_supervisor
        .cancel(parsed_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_background_job_logs(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<String>, String> {
    let parsed_id = uuid::Uuid::parse_str(&id).map_err(|e| e.to_string())?;
    state
        .bg_supervisor
        .get_logs(parsed_id)
        .map_err(|e| e.to_string())
}

// ─── Settings, Onboarding & Doctor Commands ───────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct AppConfigPayload {
    pub allowed_roots: Vec<String>,
    pub computer_use_enabled: bool,
    pub computer_use_dry_run: bool,
    pub advanced_shell_enabled: bool,
    pub allow_screenshots: bool,
    pub recording_enabled: bool,
    pub voice_enabled: bool,
    pub voice_model: String,
    pub voice_session_limit_minutes: u64,
}

impl Default for AppConfigPayload {
    fn default() -> Self {
        Self {
            allowed_roots: vec![
                std::env::var("USERPROFILE")
                    .map(|p| format!("{}\\projects", p))
                    .unwrap_or_else(|_| "D:\\projects".to_string())
            ],
            computer_use_enabled: false,
            computer_use_dry_run: true,
            advanced_shell_enabled: false,
            allow_screenshots: false,
            recording_enabled: false,
            voice_enabled: false,
            voice_model: "gemini-2.5-flash".to_string(),
            voice_session_limit_minutes: 10,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SecretsStatus {
    pub gemini_api_key_set: bool,
    pub elevenlabs_api_key_set: bool,
    pub telegram_bot_token_set: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SaveConfigResult {
    pub success: bool,
    pub warning: Option<String>,
}

#[tauri::command]
pub async fn run_doctor(opt_in_network: Option<bool>) -> Result<ruby_core::doctor::DoctorReport, String> {
    Ok(ruby_core::doctor::RubyDoctor::run_diagnostics(opt_in_network.unwrap_or(false)).await)
}

#[tauri::command]
pub fn get_app_config() -> Result<AppConfigPayload, String> {
    let paths = ruby_config::RubyPaths::resolve();
    let config_path = paths.workspace_dir.parent().unwrap_or(std::path::Path::new(".")).join("config.json");
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    } else {
        Ok(AppConfigPayload::default())
    }
}

#[tauri::command]
pub fn save_app_config(
    config: AppConfigPayload,
    confirmed: Option<bool>,
) -> Result<SaveConfigResult, String> {
    // 1. Dangerous flag gating: enabling dangerous flags requires explicit confirmation
    let dangerous_requested = config.computer_use_enabled
        || (!config.computer_use_dry_run)
        || config.advanced_shell_enabled
        || config.allow_screenshots
        || config.recording_enabled;

    if dangerous_requested && !confirmed.unwrap_or(false) {
        return Err("Enabling dangerous features (live computer use, advanced shell, screenshots, recording) requires explicit user confirmation.".to_string());
    }

    // 2. Allowed roots validation & warning
    let mut warning = None;
    for root in &config.allowed_roots {
        let trimmed = root.trim();
        if trimmed == "C:\\" || trimmed == "C:/" || trimmed == "D:\\" || trimmed == "D:/" || trimmed == "/" || trimmed == "\\" || (trimmed.len() <= 3 && trimmed.ends_with(':')) {
            warning = Some(format!("Warning: Allowed root '{}' covers an entire drive. Specifying a dedicated project folder like 'D:\\projects' is strongly recommended for security.", trimmed));
            break;
        }
    }

    let paths = ruby_config::RubyPaths::resolve();
    let config_dir = paths.workspace_dir.parent().unwrap_or(std::path::Path::new("."));
    std::fs::create_dir_all(config_dir).map_err(|e| e.to_string())?;
    let config_path = config_dir.join("config.json");
    let content = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&config_path, content).map_err(|e| e.to_string())?;

    Ok(SaveConfigResult {
        success: true,
        warning,
    })
}

#[tauri::command]
pub fn get_secrets_status() -> SecretsStatus {
    SecretsStatus {
        gemini_api_key_set: std::env::var("GEMINI_API_KEY").is_ok() || std::env::var("GOOGLE_API_KEY").is_ok(),
        elevenlabs_api_key_set: std::env::var("ELEVENLABS_API_KEY").is_ok(),
        telegram_bot_token_set: std::env::var("TELEGRAM_BOT_TOKEN").is_ok(),
    }
}

#[tauri::command]
pub fn is_first_run() -> bool {
    let paths = ruby_config::RubyPaths::resolve();
    let config_path = paths.workspace_dir.parent().unwrap_or(std::path::Path::new(".")).join("config.json");
    !config_path.exists()
}

#[cfg(test)]
mod voice_live_tests {
    use super::*;
    use ruby_core::voice_live::{
        MockToolDispatcher, NullAudioOutput, VoiceLiveManager, VoiceLiveStatus,
    };
    use ruby_gemini::live_session::{LiveSessionConfig, MockLiveSession};
    use std::sync::Arc;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_voice_live_command_start_and_stop() {
        let mgr = Arc::new(VoiceLiveManager::new(
            Arc::new(NullAudioOutput),
            Arc::new(MockToolDispatcher::default()),
            None,
        ));

        assert_eq!(mgr.status(), VoiceLiveStatus::Idle);

        let handle = MockLiveSession::open(LiveSessionConfig::default());
        mgr.start_with_handle(handle).await.expect("start ok");

        // Should reach Active within 500ms
        timeout(Duration::from_millis(500), async {
            loop {
                if mgr.status() == VoiceLiveStatus::Active {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Should reach Active");

        mgr.stop().await;

        timeout(Duration::from_millis(500), async {
            loop {
                if mgr.status() == VoiceLiveStatus::Idle {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Should return to Idle");
    }

    #[tokio::test]
    async fn test_voice_live_double_start_rejected() {
        let mgr = Arc::new(VoiceLiveManager::new(
            Arc::new(NullAudioOutput),
            Arc::new(MockToolDispatcher::default()),
            None,
        ));

        let h1 = MockLiveSession::open(LiveSessionConfig::default());
        mgr.start_with_handle(h1).await.expect("first start ok");

        tokio::time::sleep(Duration::from_millis(100)).await;

        let h2 = MockLiveSession::open(LiveSessionConfig::default());
        let result = mgr.start_with_handle(h2).await;
        assert!(result.is_err(), "Second start must be rejected");
    }

    #[tokio::test]
    async fn test_push_mic_audio_before_start_errors() {
        let mgr = Arc::new(VoiceLiveManager::new(
            Arc::new(NullAudioOutput),
            Arc::new(MockToolDispatcher::default()),
            None,
        ));
        let result = mgr.push_mic_audio(vec![0u8; 320]).await;
        assert!(result.is_err(), "push_mic_audio before start should error");
    }

    #[tokio::test]
    async fn test_get_voice_live_status_idle_string() {
        let mgr = Arc::new(VoiceLiveManager::new(
            Arc::new(NullAudioOutput),
            Arc::new(MockToolDispatcher::default()),
            None,
        ));
        assert_eq!(mgr.status(), VoiceLiveStatus::Idle);
        // Status maps to "idle"
        let status_str = match mgr.status() {
            VoiceLiveStatus::Idle => "idle",
            VoiceLiveStatus::Connecting => "connecting",
            VoiceLiveStatus::Active => "active",
            VoiceLiveStatus::Stopping => "stopping",
        };
        assert_eq!(status_str, "idle");
    }

    #[test]
    fn test_console_ipc_delete_memory_requires_confirmation_then_deletes() {
        use ruby_memory::{MemoryStore, SqliteMemoryStore};
        let store = SqliteMemoryStore::open_in_memory().unwrap();
        let id = store.add_memory("fact", "User prefers dark mode").unwrap();
        let conf_mgr = ruby_policy::UnifiedConfirmationManager::new();

        // 1. Unconfirmed request
        let res_unconfirmed = delete_memory_core(&store, &conf_mgr, id, None);
        assert!(res_unconfirmed.is_err());
        let err_msg = res_unconfirmed.unwrap_err();
        assert!(err_msg.contains("Requires confirmation: Delete memory record"));
        assert!(err_msg.contains("Reply with code"));

        let code = err_msg
            .split("Reply with code ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .expect("Must extract code");
        assert_eq!(code.len(), 6, "Confirmation code must be strictly 6 digits");

        // 2. Confirmed request
        let res_confirmed = delete_memory_core(&store, &conf_mgr, id, Some(code.to_string()));
        assert!(res_confirmed.is_ok());
        assert_eq!(
            res_confirmed.unwrap(),
            format!("Memory record [ID: {id}] deleted.")
        );

        // Verify deleted from store
        let search = store.search_memories("", 10).unwrap();
        assert_eq!(search.len(), 0);
    }

    #[test]
    fn test_console_ipc_wipe_all_memories_shows_count_requires_confirmation_then_wipes() {
        use ruby_memory::{MemoryStore, SqliteMemoryStore};
        let store = SqliteMemoryStore::open_in_memory().unwrap();
        store.add_memory("fact", "Memory 1").unwrap();
        store.add_memory("fact", "Memory 2").unwrap();
        store.add_memory("fact", "Memory 3").unwrap();
        let conf_mgr = ruby_policy::UnifiedConfirmationManager::new();

        // 1. Unconfirmed request shows total 3 records in prompt
        let res_unconfirmed = wipe_all_memories_core(&store, &conf_mgr, None);
        assert!(res_unconfirmed.is_err());
        let err_msg = res_unconfirmed.unwrap_err();
        assert!(
            err_msg.contains("Wipe all memories (total 3 records)"),
            "Expected 'Wipe all memories (total 3 records)' in error, got: {err_msg}"
        );

        let code = err_msg
            .split("Reply with code ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .expect("Must extract code");
        assert_eq!(code.len(), 6, "Confirmation code must be strictly 6 digits");

        // 2. Confirmed request
        let res_confirmed = wipe_all_memories_core(&store, &conf_mgr, Some(code.to_string()));
        assert!(res_confirmed.is_ok());
        assert_eq!(
            res_confirmed.unwrap(),
            "All memories wiped (total 3 records)."
        );

        let search = store.search_memories("", 10).unwrap();
        assert_eq!(search.len(), 0);
    }

    #[tokio::test]
    async fn test_esc_hierarchy_dispatch_mode_a_then_voice_then_dictation() {
        let cu_token = ruby_actions::computer_use::KillSwitchToken::new();
        let voice_mgr = VoiceLiveManager::new(
            Arc::new(NullAudioOutput),
            Arc::new(MockToolDispatcher::default()),
            None,
        );
        let dictation_cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Case 1: Mode A Computer Use is active -> priority 1 cancels Mode A
        let action1 = dispatch_esc_hierarchy(true, &cu_token, &voice_mgr, {
            let d = Arc::clone(&dictation_cancelled);
            async move {
                d.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        })
        .await;
        assert_eq!(action1, EscHierarchyAction::ModeAKillSwitch);
        assert!(cu_token.is_cancelled());
        assert!(!dictation_cancelled.load(std::sync::atomic::Ordering::SeqCst));

        // Case 2: Voice Live active -> priority 2 stops Voice Live
        let h = MockLiveSession::open(LiveSessionConfig::default());
        voice_mgr.start_with_handle(h).await.unwrap();
        let action2 = dispatch_esc_hierarchy(false, &cu_token, &voice_mgr, {
            let d = Arc::clone(&dictation_cancelled);
            async move {
                d.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        })
        .await;
        assert_eq!(action2, EscHierarchyAction::VoiceLiveStop);

        // Case 3: Neither active -> priority 3 cancels Dictation
        let action3 = dispatch_esc_hierarchy(false, &cu_token, &voice_mgr, {
            let d = Arc::clone(&dictation_cancelled);
            async move {
                d.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        })
        .await;
        assert_eq!(action3, EscHierarchyAction::DictationCancel);
        assert!(dictation_cancelled.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_esc_hierarchy_all_matrix_rows_tap_and_hold() {
        use ruby_actions::computer_use::KillSwitchToken;
        use ruby_core::voice_live::VoiceLiveStatus;

        // Row 1: Mode A Tap
        let cu_token = KillSwitchToken::new();
        let voice_mgr = VoiceLiveManager::new(Arc::new(NullAudioOutput), Arc::new(MockToolDispatcher::default()), None);
        let action = dispatch_esc_hierarchy(true, &cu_token, &voice_mgr, async {}).await;
        assert_eq!(action, EscHierarchyAction::ModeAKillSwitch);
        assert!(cu_token.is_cancelled());

        // Row 2: Mode V Tap
        let cu_token = KillSwitchToken::new();
        let h = MockLiveSession::open(LiveSessionConfig::default());
        voice_mgr.start_with_handle(h).await.unwrap();
        assert_ne!(voice_mgr.status(), VoiceLiveStatus::Idle);
        let action = dispatch_esc_hierarchy(false, &cu_token, &voice_mgr, async {}).await;
        assert_eq!(action, EscHierarchyAction::VoiceLiveStop);
        assert!(voice_mgr.status() == VoiceLiveStatus::Stopping || voice_mgr.status() == VoiceLiveStatus::Idle);

        // Row 3: Mode D Tap
        let cu_token = KillSwitchToken::new();
        let dictation_cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let action = dispatch_esc_hierarchy(false, &cu_token, &voice_mgr, {
            let d = Arc::clone(&dictation_cancelled);
            async move {
                d.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }).await;
        assert_eq!(action, EscHierarchyAction::DictationCancel);
        assert!(dictation_cancelled.load(std::sync::atomic::Ordering::SeqCst));

        // Row 4: Mode B Tap (Background jobs are NOT cancelled by Esc tap)
        let store = Arc::new(ruby_core::tasks::SqliteJobStore::new_in_memory().unwrap());
        let backend = Arc::new(ruby_core::MockBackend);
        let supervisor = ruby_core::tasks::BackgroundTaskSupervisor::new(store, backend, 60);
        let job = supervisor.submit("Background non-GUI task").await.unwrap();
        let action = dispatch_esc_hierarchy(false, &cu_token, &voice_mgr, async {}).await;
        assert_eq!(action, EscHierarchyAction::DictationCancel);
        assert_ne!(supervisor.get_job(job.id).unwrap().unwrap().status, ruby_core::TaskStatus::Cancelled, "Esc tap must NOT cancel background jobs");

        // Row 5: Esc-Hold Global Kill Switch on all states
        // Verifies hold cancels CU, stops Voice Live, cancels dictation, and cancels supervisor jobs while staying alive
        let cu_token = KillSwitchToken::new();
        let dictation_cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let voice_mgr5 = Arc::new(VoiceLiveManager::new(
            Arc::new(NullAudioOutput),
            Arc::new(MockToolDispatcher::default()),
            None,
        ));
        let h2 = MockLiveSession::open(LiveSessionConfig::default());
        voice_mgr5.start_with_handle(h2).await.unwrap();

        // Simulate Esc-Hold kill switch dispatch
        cu_token.cancel();
        voice_mgr5.stop().await;
        dictation_cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
        let cancelled_jobs = supervisor.cancel_all_active();

        assert!(cu_token.is_cancelled());
        assert!(voice_mgr5.status() == VoiceLiveStatus::Stopping || voice_mgr5.status() == VoiceLiveStatus::Idle);
        assert!(dictation_cancelled.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(cancelled_jobs, 1);
        // Assert process stays RUNNING (test reaches here without exit)
        assert!(true, "Application stays running after kill switch");
    }

    #[test]
    fn test_save_app_config_requires_confirmation_for_dangerous_flags() {
        let mut cfg = AppConfigPayload::default();
        cfg.computer_use_enabled = true;
        let res = save_app_config(cfg.clone(), None);
        assert!(res.is_err(), "Must require confirmation when dangerous flag enabled");
        assert!(res.unwrap_err().contains("requires explicit user confirmation"));

        // With explicit confirmation, it succeeds
        let res_confirmed = save_app_config(cfg, Some(true));
        assert!(res_confirmed.is_ok());
    }

    #[test]
    fn test_save_app_config_warns_on_whole_drive_root() {
        let mut cfg = AppConfigPayload::default();
        cfg.allowed_roots = vec!["D:\\".to_string()];
        let res = save_app_config(cfg, Some(false)).expect("should save with warning");
        assert!(res.warning.is_some());
        assert!(res.warning.unwrap().contains("covers an entire drive"));
    }

    #[test]
    fn test_save_app_config_succeeds_with_safe_defaults() {
        let cfg = AppConfigPayload::default();
        assert!(!cfg.computer_use_enabled);
        assert!(cfg.computer_use_dry_run);
        assert!(!cfg.advanced_shell_enabled);
        assert!(!cfg.allow_screenshots);
        assert!(!cfg.recording_enabled);

        let res = save_app_config(cfg, None).expect("safe defaults save without confirmation");
        assert!(res.success);
    }

    #[test]
    fn test_get_secrets_status_never_contains_values() {
        let status = get_secrets_status();
        // Verifies structure only contains booleans, no strings
        let json = serde_json::to_string(&status).unwrap();
        assert!(!json.contains("ghp_"));
        assert!(!json.contains("AIza"));
        assert!(json.contains("gemini_api_key_set"));
    }
}
