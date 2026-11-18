use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State, Window};
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_TRANSPARENT},
};

use ruby_core::{Emotion, OperationalState};

use crate::{AppState, SequencedOrbEvent};

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

#[tauri::command]
pub fn delete_memory(state: State<'_, AppState>, id: i64) -> Result<bool, String> {
    state
        .memory_store
        .delete_memory(id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn wipe_all_memories(state: State<'_, AppState>) -> Result<usize, String> {
    state.memory_store.wipe_all().map_err(|e| e.to_string())
}

#[cfg(feature = "test-capture")]
#[tauri::command]
pub fn save_orb_snapshot(name: String, base64_png: String) -> Result<String, String> {
    use base64::Engine;
    let clean = if let Some(idx) = base64_png.find("base64,") {
        &base64_png[idx + 7..]
    } else {
        &base64_png
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(clean.trim())
        .map_err(|e| e.to_string())?;

    let out_dir = std::path::PathBuf::from(r"D:\projects\ruby-agent\acceptance\tauri_rendered");
    let _ = std::fs::create_dir_all(&out_dir);
    let target = out_dir.join(format!("{}.png", name));
    std::fs::write(&target, bytes).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
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
}
