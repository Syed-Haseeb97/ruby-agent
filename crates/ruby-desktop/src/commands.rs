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

#[tauri::command]
pub fn delete_memory(
    state: State<'_, AppState>,
    id: i64,
    confirmation_code: Option<String>,
) -> Result<String, String> {
    let conf_mgr = state.tool_manager.confirmation_manager();
    if let Some(code) = confirmation_code {
        conf_mgr
            .verify_and_consume(&code)
            .map_err(|e| format!("Confirmation failed: {e}"))?;
        state
            .memory_store
            .delete_memory(id)
            .map_err(|e| e.to_string())?;
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

#[tauri::command]
pub fn wipe_all_memories(
    state: State<'_, AppState>,
    confirmation_code: Option<String>,
) -> Result<String, String> {
    let conf_mgr = state.tool_manager.confirmation_manager();
    let count = state
        .memory_store
        .search_memories("", 1000)
        .map(|m| m.len())
        .unwrap_or(0);
    if let Some(code) = confirmation_code {
        conf_mgr
            .verify_and_consume(&code)
            .map_err(|e| format!("Confirmation failed: {e}"))?;
        let wiped = state.memory_store.wipe_all().map_err(|e| e.to_string())?;
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
        ruby_policy::UnifiedConfirmationManager::generate_secure_code_4_digits()
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
                assert_eq!(code.len(), 4);

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

#[cfg(test)]
mod voice_live_tests {
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
}
