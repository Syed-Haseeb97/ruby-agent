//! Telegram channel adapter implementation.

pub mod client;
pub mod pairing;
pub mod rate_limiter;

use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, error, info, warn};

use ruby_gemini::dictation_client::redact_sensitive_content;

use crate::audit::AuditLogger;
use crate::confirmation::ConfirmationManager;
use crate::key_storage::{
    load_persisted_offset, load_remote_config, save_persisted_offset, save_remote_config,
    RemoteConfig,
};
use crate::power::get_system_power_status;
use crate::state_machine::TaskManager;
use crate::traits::{ChannelAdapter, RemoteCommandHandler};
use crate::types::{RemoteError, RemoteStatus, TaskState};

use self::client::TelegramClient;
use self::pairing::{PairingManager, PairingResult};
use self::rate_limiter::SenderRateLimiter;

pub const TELEGRAM_MAX_MESSAGE_CHARS: usize = 4000;
pub const STALENESS_THRESHOLD_SECS: u64 = 120;

pub struct TelegramAdapter {
    client: Arc<TelegramClient>,
    config: Arc<Mutex<RemoteConfig>>,
    pairing_mgr: PairingManager,
    task_mgr: TaskManager,
    confirmation_mgr: ConfirmationManager,
    audit_logger: AuditLogger,
    rate_limiter: SenderRateLimiter,
    running: Arc<AtomicBool>,
    last_command_time: Arc<Mutex<Option<u64>>>,
    handler: Arc<Mutex<Option<Arc<dyn RemoteCommandHandler>>>>,
}

impl TelegramAdapter {
    pub fn new(bot_token: String) -> Self {
        Self::with_base_url(bot_token, None)
    }

    pub fn with_base_url(bot_token: String, base_url: Option<String>) -> Self {
        let client = match base_url {
            Some(url) => Arc::new(TelegramClient::with_base_url(bot_token, url)),
            None => Arc::new(TelegramClient::new(bot_token)),
        };

        let config = Arc::new(Mutex::new(load_remote_config()));
        let pairing_mgr = PairingManager::new();
        let task_mgr = TaskManager::new();
        let confirmation_mgr = ConfirmationManager::new();
        let audit_logger = AuditLogger::new(AuditLogger::default_path());
        let rate_limiter = SenderRateLimiter::default();
        let running = Arc::new(AtomicBool::new(false));
        let last_command_time = Arc::new(Mutex::new(None));

        Self {
            client,
            config,
            pairing_mgr,
            task_mgr,
            confirmation_mgr,
            audit_logger,
            rate_limiter,
            running,
            last_command_time,
            handler: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_components(
        client: Arc<TelegramClient>,
        config: Arc<Mutex<RemoteConfig>>,
        pairing_mgr: PairingManager,
        task_mgr: TaskManager,
        confirmation_mgr: ConfirmationManager,
        audit_logger: AuditLogger,
    ) -> Self {
        Self {
            client,
            config,
            pairing_mgr,
            task_mgr,
            confirmation_mgr,
            audit_logger,
            rate_limiter: SenderRateLimiter::default(),
            running: Arc::new(AtomicBool::new(false)),
            last_command_time: Arc::new(Mutex::new(None)),
            handler: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_command_handler(&self, handler: Arc<dyn RemoteCommandHandler>) {
        *self.handler.lock().unwrap() = Some(handler);
    }

    pub fn task_manager(&self) -> TaskManager {
        self.task_mgr.clone()
    }

    pub fn confirmation_manager(&self) -> ConfirmationManager {
        self.confirmation_mgr.clone()
    }

    pub fn pairing_manager(&self) -> PairingManager {
        self.pairing_mgr.clone()
    }

    /// Disconnects and unpairs the current remote.
    pub fn disconnect_remote(&self) -> Result<(), String> {
        let mut lock = self.config.lock().map_err(|e| e.to_string())?;
        lock.paired_user_id = None;
        save_remote_config(&lock)?;
        self.pairing_mgr.clear();
        self.running.store(false, Ordering::SeqCst);
        info!("Remote disconnected and user unpaired by request.");
        Ok(())
    }

    /// Returns the current remote status.
    pub fn get_status(&self) -> RemoteStatus {
        let lock = self.config.lock().ok();
        let paired_user_id = lock.as_ref().and_then(|c| c.paired_user_id);
        let paired = paired_user_id.is_some();
        let enabled = lock.as_ref().map(|c| c.enabled).unwrap_or(false);

        let (active_pairing_code, pairing_code_expires_in_secs) = if !paired {
            let (code, remaining) = self.pairing_mgr.get_or_generate_code();
            (Some(code), Some(remaining))
        } else {
            (None, None)
        };

        RemoteStatus {
            enabled,
            paired,
            paired_user_id,
            connected: self.running.load(Ordering::SeqCst),
            active_pairing_code,
            pairing_code_expires_in_secs,
            last_command_time_secs: self.last_command_time.lock().ok().and_then(|t| *t),
        }
    }

    /// Applies secret redaction and sends. If text exceeds 4000 chars, sends preview + document upload.
    async fn send_redacted_reply(&self, chat_id: i64, text: &str) -> Result<(), RemoteError> {
        let sanitized = redact_sensitive_content(text, None);
        if sanitized.len() <= TELEGRAM_MAX_MESSAGE_CHARS {
            self.client.send_message(chat_id, &sanitized).await?;
        } else {
            let preview = format!(
                "{}\n\n[... output exceeds {} characters; full text attached below as output.txt]",
                &sanitized[..std::cmp::min(sanitized.len(), 1000)],
                TELEGRAM_MAX_MESSAGE_CHARS
            );
            let _ = self.client.send_message(chat_id, &preview).await;
            let _ = self
                .client
                .send_document(
                    chat_id,
                    "output.txt",
                    sanitized.as_bytes(),
                    Some("Full command output"),
                )
                .await;
        }
        Ok(())
    }

    /// Dispatches a single incoming Telegram update.
    pub async fn process_update(
        &self,
        update: serde_json::Value,
        now_secs: u64,
    ) -> Result<Option<i64>, RemoteError> {
        let update_id = match update["update_id"].as_i64() {
            Some(id) => id,
            None => return Ok(None),
        };

        let message = &update["message"];
        if message.is_null() {
            return Ok(Some(update_id));
        }

        // 1. Strict chat type filter: Only private chats allowed
        let chat_type = message["chat"]["type"].as_str().unwrap_or("");
        if chat_type != "private" {
            debug!("Ignoring non-private message (chat type: '{}')", chat_type);
            return Ok(Some(update_id));
        }

        let sender_id = match message["from"]["id"].as_i64() {
            Some(id) => id,
            None => return Ok(Some(update_id)),
        };

        let chat_id = match message["chat"]["id"].as_i64() {
            Some(id) => id,
            None => return Ok(Some(update_id)),
        };

        let message_date = message["date"].as_u64().unwrap_or(0);
        let raw_text = message["text"].as_str().unwrap_or("").trim();

        let paired_user_id = self.config.lock().ok().and_then(|c| c.paired_user_id);

        // 2. Pairing Gate
        if paired_user_id.is_none() {
            // Unpaired mode: Check for /pair <code>
            if raw_text.starts_with("/pair") {
                let parts: Vec<&str> = raw_text.split_whitespace().collect();
                if parts.len() == 2 {
                    let code = parts[1];
                    match self.pairing_mgr.validate_code(code) {
                        PairingResult::Success => {
                            if let Ok(mut lock) = self.config.lock() {
                                lock.paired_user_id = Some(sender_id);
                                let _ = save_remote_config(&lock);
                            }
                            self.audit_logger
                                .log_entry(sender_id, None, "pair", "success");
                            info!(
                                "Telegram bot paired successfully with user ID: {}",
                                sender_id
                            );
                            self.send_redacted_reply(
                                chat_id,
                                "✅ Paired successfully with Ruby Assistant.\nUse /help to view available commands.",
                            )
                            .await?;
                            return Ok(Some(update_id));
                        }
                        PairingResult::MaxAttemptsExceeded => {
                            self.audit_logger.log_entry(
                                sender_id,
                                None,
                                "pair",
                                "invalidated_max_attempts_exceeded",
                            );
                            warn!(
                                "Pairing code invalidated after 5 failed attempts from sender ID: {}",
                                sender_id
                            );
                            self.send_redacted_reply(
                                chat_id,
                                "❌ Pairing code invalidated after 5 failed attempts. Please generate a new code in the Ruby Assistant Console.",
                            )
                            .await?;
                            return Ok(Some(update_id));
                        }
                        PairingResult::InvalidOrExpired => {
                            self.audit_logger.log_entry(
                                sender_id,
                                None,
                                "pair",
                                "invalid_or_expired_code",
                            );
                            warn!(
                                "Rejected invalid or expired pairing attempt from sender ID: {}",
                                sender_id
                            );
                            self.send_redacted_reply(
                                chat_id,
                                "❌ Invalid or expired pairing code. Please generate a fresh code in the Ruby Assistant Console.",
                            )
                            .await?;
                            return Ok(Some(update_id));
                        }
                    }
                }
                // Malformed /pair command
                self.audit_logger
                    .log_entry(sender_id, None, "pair", "invalid_or_expired_code");
                warn!(
                    "Rejected malformed pairing attempt from sender ID: {}",
                    sender_id
                );
                self.send_redacted_reply(
                    chat_id,
                    "❌ Invalid or expired pairing code. Please generate a fresh code in the Ruby Assistant Console.",
                )
                .await?;
                return Ok(Some(update_id));
            }

            // In unpaired mode, silently ignore any other command to prevent information leakage
            return Ok(Some(update_id));
        }

        // 3. Authorization check: Drop every message from unauthorized user IDs silently (no reply)
        let authorized_user_id = paired_user_id.unwrap();
        if sender_id != authorized_user_id {
            if self.rate_limiter.should_log(sender_id) {
                warn!("Ignored message from unauthorized sender ID: {}", sender_id);
            }
            self.audit_logger
                .log_entry(sender_id, None, "unauthorized", "dropped_silently");
            return Ok(Some(update_id));
        }

        // Update last command time
        if let Ok(mut t) = self.last_command_time.lock() {
            *t = Some(now_secs);
        }

        // 4. Staleness Gate: Drop messages older than 120s at receipt
        if now_secs.saturating_sub(message_date) > STALENESS_THRESHOLD_SECS {
            self.audit_logger.log_entry(
                sender_id,
                None,
                "stale_message",
                "dropped_offline_reply_sent",
            );
            warn!(
                "Dropped stale message from sender ID: {} (age: {}s > 120s threshold)",
                sender_id,
                now_secs.saturating_sub(message_date)
            );
            self.send_redacted_reply(chat_id, "Laptop was offline — resend if still needed")
                .await?;
            return Ok(Some(update_id));
        }

        // 5. Command Parsing & Execution
        let (cmd, args) = if raw_text.starts_with('/') {
            let mut parts = raw_text.splitn(2, ' ');
            let command = parts.next().unwrap_or("").to_ascii_lowercase();
            let remainder = parts.next().unwrap_or("").trim();
            (command, remainder)
        } else {
            ("plain_text".to_string(), raw_text)
        };

        match cmd.as_str() {
            "/help" => {
                let help_text = "Ruby Assistant Remote (Phase R1 Read-Only)\n\n\
                Available Commands:\n\
                • /status - Laptop power, battery, uptime & active task status\n\
                • /tasks - View recent tasks and current states\n\
                • /stop - Immediately cancel all active and queued tasks\n\
                • /ag [id] - Inspect Antigravity agent conversations & artifact summaries\n\
                • /yes <code> - Confirm pending escalated action\n\
                • /no - Cancel pending escalated action\n\
                • /help - Show this command list\n\
                • Read-only tools (inside allowed roots):\n\
                  - list <path> / ls <path>\n\
                  - read <path> / cat <path> (capped at 64KB, binary refused)\n\
                  - info <path> / stat <path>\n\
                  - send <path> / fetch <path> (document upload capped at 10MB)\n\
                  - git status / git log / git diff\n\
                  - gh pr list / gh issue list";
                self.audit_logger
                    .log_entry(sender_id, None, "help", "success");
                self.send_redacted_reply(chat_id, help_text).await?;
            }
            "/ag" => {
                let ag_reply = crate::antigravity::inspect_antigravity_brain(if args.is_empty() {
                    None
                } else {
                    Some(args)
                });
                self.audit_logger
                    .log_entry(sender_id, None, "ag", "success");
                self.send_redacted_reply(chat_id, &ag_reply).await?;
            }
            "/status" => {
                let power = get_system_power_status();
                let active_tasks = self.task_mgr.active_task_count();
                let status_reply = format!(
                    "Ruby Laptop Status:\n\
                    • State: Active & Polling\n\
                    • Power: {}\n\
                    • Uptime: {}\n\
                    • Active Tasks: {}",
                    power.battery_status_desc, power.uptime_desc, active_tasks
                );
                self.audit_logger
                    .log_entry(sender_id, None, "status", "success");
                self.send_redacted_reply(chat_id, &status_reply).await?;
            }
            "/tasks" => {
                let recent = self.task_mgr.get_recent_tasks(5);
                let reply = if recent.is_empty() {
                    "No recent tasks recorded.".to_string()
                } else {
                    let mut lines = vec!["Recent Remote Tasks:".to_string()];
                    for t in recent {
                        lines.push(format!(
                            "• [{}] State: {} | Prompt: \"{}\"",
                            t.id,
                            t.state.as_str(),
                            t.raw_prompt
                        ));
                    }
                    lines.join("\n")
                };
                self.audit_logger
                    .log_entry(sender_id, None, "tasks", "success");
                self.send_redacted_reply(chat_id, &reply).await?;
            }
            "/stop" => {
                let cancelled_count = self.task_mgr.cancel_all_active();
                let pending_action = self.confirmation_mgr.cancel_pending();
                self.audit_logger
                    .log_entry(sender_id, None, "stop", "all_cancelled");
                let reply = if cancelled_count > 0 || pending_action.is_some() {
                    format!(
                        "🛑 Cancelled {} active task(s) and cleared pending actions.",
                        cancelled_count
                    )
                } else {
                    "🛑 Stop confirmed. No active tasks were running.".to_string()
                };
                self.send_redacted_reply(chat_id, &reply).await?;
            }
            "/yes" => {
                let code = args;
                if code.is_empty() {
                    self.send_redacted_reply(chat_id, "Usage: /yes <code> (e.g. /yes 4821)")
                        .await?;
                } else {
                    match self.confirmation_mgr.verify_code(code) {
                        Ok(action) => {
                            self.audit_logger
                                .log_entry(sender_id, None, "yes", "confirmed");
                            let recent = self.task_mgr.get_recent_tasks(5);
                            if let Some(t) = recent.iter().find(|t| {
                                t.sender_id == sender_id && t.state == TaskState::NeedsInput
                            }) {
                                self.task_mgr.update_task_state(
                                    &t.id,
                                    TaskState::Done,
                                    Some(format!("Confirmed: {}", action.description)),
                                    None,
                                );
                            }
                            let reply = format!("✅ Confirmed action: {}", action.description);
                            self.send_redacted_reply(chat_id, &reply).await?;
                        }
                        Err(e) => {
                            self.audit_logger
                                .log_entry(sender_id, None, "yes", "rejected");
                            let reply = format!("❌ {}", e);
                            self.send_redacted_reply(chat_id, &reply).await?;
                        }
                    }
                }
            }
            "/no" => {
                let cancelled = self.confirmation_mgr.cancel_pending();
                self.audit_logger
                    .log_entry(sender_id, None, "no", "cancelled");
                let recent = self.task_mgr.get_recent_tasks(5);
                if let Some(t) = recent
                    .iter()
                    .find(|t| t.sender_id == sender_id && t.state == TaskState::NeedsInput)
                {
                    self.task_mgr.update_task_state(
                        &t.id,
                        TaskState::Cancelled,
                        None,
                        Some("User cancelled pending confirmation".into()),
                    );
                }
                let reply = match cancelled {
                    Some(act) => format!("❌ Rejected action: {}", act.description),
                    None => "❌ No pending action to reject.".to_string(),
                };
                self.send_redacted_reply(chat_id, &reply).await?;
            }
            "plain_text" => {
                let task = self.task_mgr.create_task(sender_id, args);
                let incoming = crate::types::IncomingMessage {
                    update_id,
                    message_id: message["message_id"].as_i64().unwrap_or(0),
                    sender_id,
                    chat_id,
                    text: args.to_string(),
                    timestamp_secs: message_date,
                    is_private: true,
                };
                let handler_opt = self.handler.lock().unwrap().clone();
                let reply = if let Some(h) = handler_opt {
                    self.task_mgr
                        .update_task_state(&task.id, TaskState::Running, None, None);
                    match h.handle_command(incoming).await {
                        Ok(res) => {
                            if res.contains("requires confirmation") {
                                self.task_mgr.update_task_state(
                                    &task.id,
                                    TaskState::NeedsInput,
                                    Some(res.clone()),
                                    None,
                                );
                            } else {
                                self.task_mgr.update_task_state(
                                    &task.id,
                                    TaskState::Done,
                                    Some(res.clone()),
                                    None,
                                );
                            }
                            self.audit_logger.log_entry(
                                sender_id,
                                Some(&task.id),
                                "plain_text",
                                "task_completed",
                            );
                            if let Some(path_to_send) = res.strip_prefix("READY_FOR_UPLOAD:") {
                                let p = std::path::Path::new(path_to_send);
                                if let Ok(bytes) = std::fs::read(p) {
                                    let filename = p
                                        .file_name()
                                        .and_then(|f| f.to_str())
                                        .unwrap_or("document.bin");
                                    let _ = self
                                        .client
                                        .send_document(
                                            chat_id,
                                            filename,
                                            &bytes,
                                            Some(&format!("Document: {filename}")),
                                        )
                                        .await;
                                    format!("Sent document: {filename} ({} bytes)", bytes.len())
                                } else {
                                    "Failed to read document for upload".to_string()
                                }
                            } else {
                                res
                            }
                        }
                        Err(e) => {
                            let err_msg = format!("Task failed: {}", e);
                            self.task_mgr.update_task_state(
                                &task.id,
                                TaskState::Failed,
                                None,
                                Some(err_msg.clone()),
                            );
                            self.audit_logger.log_entry(
                                sender_id,
                                Some(&task.id),
                                "plain_text",
                                "task_failed",
                            );
                            err_msg
                        }
                    }
                } else {
                    self.task_mgr.update_task_state(
                        &task.id,
                        TaskState::Done,
                        Some("received, tool execution isn't enabled yet".to_string()),
                        None,
                    );
                    self.audit_logger.log_entry(
                        sender_id,
                        Some(&task.id),
                        "plain_text",
                        "task_created_done",
                    );
                    format!("[{}] received, tool execution isn't enabled yet", task.id)
                };
                self.send_redacted_reply(chat_id, &reply).await?;
            }
            other => {
                let reply = format!(
                    "Unknown command '{}'. Send /help for available commands.",
                    other
                );
                self.send_redacted_reply(chat_id, &reply).await?;
            }
        }

        Ok(Some(update_id))
    }
}

#[async_trait]
impl ChannelAdapter for TelegramAdapter {
    fn name(&self) -> &str {
        "telegram"
    }

    async fn start_polling(
        &self,
        handler: Arc<dyn RemoteCommandHandler>,
    ) -> Result<(), RemoteError> {
        self.running.store(true, Ordering::SeqCst);
        *self.handler.lock().unwrap() = Some(Arc::clone(&handler));
        let client = Arc::clone(&self.client);
        let running = Arc::clone(&self.running);

        let mut offset = load_persisted_offset();
        info!(
            "Starting Telegram long polling with initial offset: {}",
            offset
        );

        while running.load(Ordering::SeqCst) {
            let res = client.get_updates(Some(offset), 25).await;
            match res {
                Ok(val) => {
                    if let Some(updates) = val["result"].as_array() {
                        let now_secs = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();

                        for update in updates {
                            match self.process_update(update.clone(), now_secs).await {
                                Ok(Some(up_id)) => {
                                    offset = up_id + 1;
                                    let _ = save_persisted_offset(offset);
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    error!("Error processing Telegram update: {:?}", e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    if !running.load(Ordering::SeqCst) {
                        break;
                    }
                    warn!("Telegram getUpdates network error: {:?}; retrying in 5s", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
        }

        info!("Telegram long polling stopped cleanly.");
        Ok(())
    }

    async fn send_reply(&self, chat_id: i64, text: &str) -> Result<(), RemoteError> {
        self.send_redacted_reply(chat_id, text).await
    }

    async fn send_file(
        &self,
        chat_id: i64,
        filename: &str,
        data: &[u8],
        caption: Option<&str>,
    ) -> Result<(), RemoteError> {
        let cap_redacted = caption.map(|c| redact_sensitive_content(c, None));
        self.client
            .send_document(chat_id, filename, data, cap_redacted.as_deref())
            .await?;
        Ok(())
    }

    async fn stop(&self) -> Result<(), RemoteError> {
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
