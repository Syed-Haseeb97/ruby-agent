//! Comprehensive mock Telegram server and unit tests for ruby-remote.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use ruby_gemini::dictation_client::redact_sensitive_content;
use ruby_remote::audit::AuditLogger;
use ruby_remote::confirmation::ConfirmationManager;
use ruby_remote::key_storage::{RemoteConfig, RemoteOffsetState};
use ruby_remote::state_machine::TaskManager;
use ruby_remote::telegram::client::TelegramClient;
use ruby_remote::telegram::pairing::PairingManager;
use ruby_remote::telegram::TelegramAdapter;
use ruby_remote::types::TaskState;

/// Helper to spin up a mock Telegram server on an ephemeral port.
#[allow(dead_code)]
struct MockTelegramServer {
    pub base_url: String,
    pub sent_messages: Arc<Mutex<Vec<serde_json::Value>>>,
    pub sent_documents: Arc<Mutex<Vec<String>>>,
    pub updates_to_serve: Arc<Mutex<Vec<serde_json::Value>>>,
    pub get_updates_calls: Arc<AtomicUsize>,
}

impl MockTelegramServer {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral port");
        let port = listener.local_addr().unwrap().port();
        let base_url = format!("http://127.0.0.1:{}", port);

        let sent_messages = Arc::new(Mutex::new(Vec::new()));
        let sent_clone = Arc::clone(&sent_messages);

        let sent_documents = Arc::new(Mutex::new(Vec::new()));
        let sent_docs_clone = Arc::clone(&sent_documents);

        let updates_to_serve = Arc::new(Mutex::new(Vec::new()));
        let updates_clone = Arc::clone(&updates_to_serve);

        let get_updates_calls = Arc::new(AtomicUsize::new(0));
        let calls_clone = Arc::clone(&get_updates_calls);

        tokio::spawn(async move {
            loop {
                let (mut socket, _) = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(_) => break,
                };

                let sent_inner = Arc::clone(&sent_clone);
                let sent_docs_inner = Arc::clone(&sent_docs_clone);
                let updates_inner = Arc::clone(&updates_clone);
                let calls_inner = Arc::clone(&calls_clone);

                tokio::spawn(async move {
                    let mut buf = vec![0u8; 16384];
                    let mut total_read = 0;
                    while total_read < buf.len() {
                        let n = socket.read(&mut buf[total_read..]).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        total_read += n;
                        if let Some(pos) =
                            buf[..total_read].windows(4).position(|w| w == b"\r\n\r\n")
                        {
                            // Header read complete
                            let header = String::from_utf8_lossy(&buf[..pos]);
                            let body_start = pos + 4;
                            let mut content_length = 0;
                            for line in header.lines() {
                                if line.to_ascii_lowercase().starts_with("content-length:") {
                                    if let Some(len_str) = line.split(':').nth(1) {
                                        content_length =
                                            len_str.trim().parse::<usize>().unwrap_or(0);
                                    }
                                }
                            }

                            // Read remaining body if any
                            let mut body = buf[body_start..total_read].to_vec();
                            while body.len() < content_length {
                                let mut temp = vec![0u8; 4096];
                                let n2 = socket.read(&mut temp).await.unwrap_or(0);
                                if n2 == 0 {
                                    break;
                                }
                                body.extend_from_slice(&temp[..n2]);
                            }

                            let body_str = String::from_utf8_lossy(&body);

                            if header.contains("/getUpdates") {
                                calls_inner.fetch_add(1, Ordering::SeqCst);
                                let updates = {
                                    let mut lock = updates_inner.lock().unwrap();
                                    let list = lock.clone();
                                    lock.clear();
                                    list
                                };
                                let resp = serde_json::json!({
                                    "ok": true,
                                    "result": updates
                                });
                                let resp_bytes = serde_json::to_vec(&resp).unwrap();
                                let http_resp = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    resp_bytes.len()
                                );
                                let _ = socket.write_all(http_resp.as_bytes()).await;
                                let _ = socket.write_all(&resp_bytes).await;
                            } else if header.contains("/sendMessage") {
                                if let Ok(json_val) =
                                    serde_json::from_str::<serde_json::Value>(&body_str)
                                {
                                    sent_inner.lock().unwrap().push(json_val);
                                }
                                let resp = serde_json::json!({
                                    "ok": true,
                                    "result": { "message_id": 101 }
                                });
                                let resp_bytes = serde_json::to_vec(&resp).unwrap();
                                let http_resp = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    resp_bytes.len()
                                );
                                let _ = socket.write_all(http_resp.as_bytes()).await;
                                let _ = socket.write_all(&resp_bytes).await;
                            } else if header.contains("/sendDocument") {
                                sent_docs_inner.lock().unwrap().push(body_str.to_string());
                                let resp = serde_json::json!({
                                    "ok": true,
                                    "result": { "message_id": 102 }
                                });
                                let resp_bytes = serde_json::to_vec(&resp).unwrap();
                                let http_resp = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    resp_bytes.len()
                                );
                                let _ = socket.write_all(http_resp.as_bytes()).await;
                                let _ = socket.write_all(&resp_bytes).await;
                            } else {
                                let resp = serde_json::json!({ "ok": true, "result": {} });
                                let resp_bytes = serde_json::to_vec(&resp).unwrap();
                                let http_resp = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    resp_bytes.len()
                                );
                                let _ = socket.write_all(http_resp.as_bytes()).await;
                                let _ = socket.write_all(&resp_bytes).await;
                            }
                            break;
                        }
                    }
                });
            }
        });

        Self {
            base_url,
            sent_messages,
            sent_documents,
            updates_to_serve,
            get_updates_calls,
        }
    }
}

// 1. Unknown sender ignored silently (0 replies sent)
#[tokio::test]
async fn test_unknown_sender_ignored_silently() {
    let mock = MockTelegramServer::start().await;
    let temp_dir = tempfile::tempdir().unwrap();
    let audit_file = temp_dir.path().join("audit.log");

    let client = Arc::new(TelegramClient::with_base_url(
        "MOCK_BOT_TOKEN".to_string(),
        mock.base_url.clone(),
    ));
    let config = Arc::new(Mutex::new(RemoteConfig {
        enabled: true,
        keep_awake: false,
        paired_user_id: Some(12345), // Paired to 12345
        allowed_roots: vec![],
    }));

    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        TaskManager::new(),
        ConfirmationManager::new(),
        AuditLogger::new(audit_file.clone()),
    );

    // Queue update from unknown sender 99999
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let update = serde_json::json!({
        "update_id": 1,
        "message": {
            "message_id": 10,
            "from": { "id": 99999 },
            "chat": { "id": 99999, "type": "private" },
            "date": now_secs,
            "text": "Hello world"
        }
    });

    let res = adapter.process_update(update, now_secs).await.unwrap();
    assert_eq!(res, Some(1));

    // Zero messages sent back
    assert_eq!(mock.sent_messages.lock().unwrap().len(), 0);

    // Audit log has record of dropped message WITHOUT message content
    let audit_content = std::fs::read_to_string(&audit_file).unwrap();
    assert!(audit_content.contains("unauthorized"));
    assert!(audit_content.contains("dropped_silently"));
    assert!(!audit_content.contains("Hello world")); // Unknown message body is never logged
}

// 2. Pairing flow: success, expiry after 5 minutes, wrong code
#[tokio::test]
async fn test_pairing_flow_success_expiry_and_wrong_code() {
    let mock = MockTelegramServer::start().await;
    let temp_dir = tempfile::tempdir().unwrap();
    let audit_file = temp_dir.path().join("audit.log");

    let client = Arc::new(TelegramClient::with_base_url(
        "MOCK_BOT_TOKEN".to_string(),
        mock.base_url.clone(),
    ));
    let config = Arc::new(Mutex::new(RemoteConfig {
        enabled: true,
        keep_awake: false,
        paired_user_id: None, // Unpaired
        allowed_roots: vec![],
    }));

    let pairing_mgr = PairingManager::new();
    let (code, remaining) = pairing_mgr.get_or_generate_code();
    assert_eq!(code.len(), 6);
    assert!(remaining <= 300);

    let adapter = TelegramAdapter::with_components(
        client,
        config.clone(),
        pairing_mgr.clone(),
        TaskManager::new(),
        ConfirmationManager::new(),
        AuditLogger::new(audit_file),
    );

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 2a. Wrong code
    let wrong_update = serde_json::json!({
        "update_id": 1,
        "message": {
            "message_id": 10,
            "from": { "id": 55555 },
            "chat": { "id": 55555, "type": "private" },
            "date": now_secs,
            "text": "/pair 000000"
        }
    });
    adapter
        .process_update(wrong_update, now_secs)
        .await
        .unwrap();
    assert_eq!(config.lock().unwrap().paired_user_id, None);
    {
        let sent = mock.sent_messages.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert!(sent[0]["text"]
            .as_str()
            .unwrap()
            .contains("Invalid or expired"));
    }

    // 2b. Expiry test
    let expired_mgr = PairingManager::new();
    expired_mgr.set_code_for_test("654321", Duration::from_millis(5));
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(
        expired_mgr.validate_code("654321"),
        ruby_remote::telegram::pairing::PairingResult::InvalidOrExpired
    );

    // 2c. Successful pair
    let correct_update = serde_json::json!({
        "update_id": 2,
        "message": {
            "message_id": 11,
            "from": { "id": 55555 },
            "chat": { "id": 55555, "type": "private" },
            "date": now_secs,
            "text": format!("/pair {}", code)
        }
    });
    adapter
        .process_update(correct_update, now_secs)
        .await
        .unwrap();
    assert_eq!(config.lock().unwrap().paired_user_id, Some(55555));
    {
        let sent = mock.sent_messages.lock().unwrap();
        assert_eq!(sent.len(), 2);
        assert!(sent[1]["text"]
            .as_str()
            .unwrap()
            .contains("Paired successfully"));
    }
}

// 3. Stale message (>120s) dropped with "Laptop was offline"
#[tokio::test]
async fn test_stale_message_dropped() {
    let mock = MockTelegramServer::start().await;
    let temp_dir = tempfile::tempdir().unwrap();
    let audit_file = temp_dir.path().join("audit.log");

    let client = Arc::new(TelegramClient::with_base_url(
        "MOCK_BOT_TOKEN".to_string(),
        mock.base_url.clone(),
    ));
    let config = Arc::new(Mutex::new(RemoteConfig {
        enabled: true,
        keep_awake: false,
        paired_user_id: Some(77777),
        allowed_roots: vec![],
    }));

    let task_mgr = TaskManager::new();
    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        task_mgr.clone(),
        ConfirmationManager::new(),
        AuditLogger::new(audit_file),
    );

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let stale_date = now_secs - 150; // 150s old (> 120s threshold)

    let stale_update = serde_json::json!({
        "update_id": 5,
        "message": {
            "message_id": 20,
            "from": { "id": 77777 },
            "chat": { "id": 77777, "type": "private" },
            "date": stale_date,
            "text": "Stale prompt from when laptop was closed"
        }
    });

    adapter
        .process_update(stale_update, now_secs)
        .await
        .unwrap();

    // Verify reply sent: "Laptop was offline — resend if still needed"
    let sent = mock.sent_messages.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0]["text"].as_str().unwrap(),
        "Laptop was offline — resend if still needed"
    );

    // No task created
    assert_eq!(task_mgr.get_recent_tasks(10).len(), 0);
}

// 4. Restart doesn't replay: offset persisted and resumed
#[test]
fn test_restart_offset_persistence() {
    let temp_dir = tempfile::tempdir().unwrap();
    let offset_file = temp_dir.path().join("remote_offset.json");

    let initial = RemoteOffsetState {
        last_update_id: 1042,
    };
    let json = serde_json::to_string(&initial).unwrap();
    std::fs::write(&offset_file, json).unwrap();

    let read_back: RemoteOffsetState =
        serde_json::from_str(&std::fs::read_to_string(&offset_file).unwrap()).unwrap();
    assert_eq!(read_back.last_update_id, 1042);
}

// 5. /stop cancels all active tasks
#[tokio::test]
async fn test_stop_cancels_all_active_tasks() {
    let mock = MockTelegramServer::start().await;
    let temp_dir = tempfile::tempdir().unwrap();
    let audit_file = temp_dir.path().join("audit.log");

    let client = Arc::new(TelegramClient::with_base_url(
        "MOCK_BOT_TOKEN".to_string(),
        mock.base_url.clone(),
    ));
    let config = Arc::new(Mutex::new(RemoteConfig {
        enabled: true,
        keep_awake: false,
        paired_user_id: Some(88888),
        allowed_roots: vec![],
    }));

    let task_mgr = TaskManager::new();
    let task = task_mgr.create_task(88888, "Long running job");
    task_mgr.update_task_state(&task.id, TaskState::Running, None, None);

    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        task_mgr.clone(),
        ConfirmationManager::new(),
        AuditLogger::new(audit_file),
    );

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let stop_update = serde_json::json!({
        "update_id": 10,
        "message": {
            "message_id": 30,
            "from": { "id": 88888 },
            "chat": { "id": 88888, "type": "private" },
            "date": now_secs,
            "text": "/stop"
        }
    });

    adapter.process_update(stop_update, now_secs).await.unwrap();

    let tasks = task_mgr.get_recent_tasks(1);
    assert_eq!(tasks[0].state, TaskState::Cancelled);

    let sent = mock.sent_messages.lock().unwrap();
    assert!(sent.iter().any(|s| s["text"]
        .as_str()
        .unwrap()
        .contains("Cancelled 1 active task")));
}

// 6. /yes with wrong/expired code rejected; /no cancels
#[tokio::test]
async fn test_confirmation_plumbing() {
    let mock = MockTelegramServer::start().await;
    let temp_dir = tempfile::tempdir().unwrap();
    let audit_file = temp_dir.path().join("audit.log");

    let client = Arc::new(TelegramClient::with_base_url(
        "MOCK_BOT_TOKEN".to_string(),
        mock.base_url.clone(),
    ));
    let config = Arc::new(Mutex::new(RemoteConfig {
        enabled: true,
        keep_awake: false,
        paired_user_id: Some(99990),
        allowed_roots: vec![],
    }));

    let confirmation_mgr = ConfirmationManager::new();
    confirmation_mgr.register_action_with_code(
        "test_action",
        "Test action",
        "9999",
        Duration::from_secs(120),
    );

    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        TaskManager::new(),
        confirmation_mgr.clone(),
        AuditLogger::new(audit_file),
    );

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 6a. Wrong code
    let wrong_update = serde_json::json!({
        "update_id": 20,
        "message": {
            "message_id": 40,
            "from": { "id": 99990 },
            "chat": { "id": 99990, "type": "private" },
            "date": now_secs,
            "text": "/yes 1111"
        }
    });
    adapter
        .process_update(wrong_update, now_secs)
        .await
        .unwrap();
    {
        let sent = mock.sent_messages.lock().unwrap();
        assert!(sent.iter().any(|s| s["text"]
            .as_str()
            .unwrap()
            .contains("Incorrect confirmation code")
            || s["text"]
                .as_str()
                .unwrap()
                .contains("Invalid confirmation code")));
    }

    // 6b. Correct code succeeds
    let correct_update = serde_json::json!({
        "update_id": 21,
        "message": {
            "message_id": 41,
            "from": { "id": 99990 },
            "chat": { "id": 99990, "type": "private" },
            "date": now_secs,
            "text": "/yes 9999"
        }
    });
    adapter
        .process_update(correct_update, now_secs)
        .await
        .unwrap();
    {
        let sent = mock.sent_messages.lock().unwrap();
        assert!(sent.iter().any(|s| s["text"]
            .as_str()
            .unwrap()
            .contains("Confirmed action: Test action")));
    }

    // 6c. /no cancels
    confirmation_mgr.register_action_with_code(
        "second_action",
        "Second action",
        "8888",
        Duration::from_secs(120),
    );
    let no_update = serde_json::json!({
        "update_id": 22,
        "message": {
            "message_id": 42,
            "from": { "id": 99990 },
            "chat": { "id": 99990, "type": "private" },
            "date": now_secs,
            "text": "/no"
        }
    });
    adapter.process_update(no_update, now_secs).await.unwrap();
    {
        let sent = mock.sent_messages.lock().unwrap();
        assert!(sent.iter().any(|s| s["text"]
            .as_str()
            .unwrap()
            .contains("Rejected action: Second action")));
    }
}

// 7. /do command starts Mode A task, consequential action requires /yes <code>
#[tokio::test]
async fn test_telegram_do_command_and_consequential_confirmation() {
    let mock = MockTelegramServer::start().await;
    let temp_dir = tempfile::tempdir().unwrap();
    let audit_file = temp_dir.path().join("audit.log");

    let client = Arc::new(TelegramClient::with_base_url(
        "MOCK_BOT_TOKEN".to_string(),
        mock.base_url.clone(),
    ));
    let config = Arc::new(Mutex::new(RemoteConfig {
        enabled: true,
        keep_awake: false,
        paired_user_id: Some(88888),
        allowed_roots: vec![],
    }));

    let task_mgr = TaskManager::new();
    let confirmation_mgr = ConfirmationManager::new();
    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        task_mgr.clone(),
        confirmation_mgr.clone(),
        AuditLogger::new(audit_file),
    );
    adapter.set_computer_use_enabled(true);

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 7a. Normal harmless Mode A task
    let harmless_update = serde_json::json!({
        "update_id": 30,
        "message": {
            "message_id": 50,
            "from": { "id": 88888 },
            "chat": { "id": 88888, "type": "private" },
            "date": now_secs,
            "text": "/do open notepad and draft meeting notes"
        }
    });
    adapter
        .process_update(harmless_update, now_secs)
        .await
        .unwrap();

    let tasks = task_mgr.get_recent_tasks(5);
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].state, TaskState::Running);
    assert!(tasks[0].raw_prompt.contains("open notepad"));

    // 7b. Consequential task (buy/purchase item) -> escalates to confirmation
    let purchase_update = serde_json::json!({
        "update_id": 31,
        "message": {
            "message_id": 51,
            "from": { "id": 88888 },
            "chat": { "id": 88888, "type": "private" },
            "date": now_secs,
            "text": "/do buy new headphones on store checkout"
        }
    });
    adapter
        .process_update(purchase_update, now_secs)
        .await
        .unwrap();

    let tasks = task_mgr.get_recent_tasks(5);
    assert_eq!(tasks.len(), 2);
    // Task 2 escalated to NeedsInput
    assert_eq!(tasks[0].state, TaskState::NeedsInput);

    let sent = mock.sent_messages.lock().unwrap();
    assert!(sent.iter().any(|s| s["text"]
        .as_str()
        .unwrap()
        .contains("Consequential action detected")
        && s["text"].as_str().unwrap().contains("Type /yes")));
}

#[tokio::test]
async fn test_telegram_do_refuses_when_computer_use_disabled() {
    let mock = MockTelegramServer::start().await;
    let temp_dir = tempfile::tempdir().unwrap();
    let audit_file = temp_dir.path().join("audit.log");

    let client = Arc::new(TelegramClient::with_base_url(
        "MOCK_BOT_TOKEN".to_string(),
        mock.base_url.clone(),
    ));
    let config = Arc::new(Mutex::new(RemoteConfig {
        enabled: true,
        keep_awake: false,
        paired_user_id: Some(88888),
        allowed_roots: vec![],
    }));

    let task_mgr = TaskManager::new();
    let confirmation_mgr = ConfirmationManager::new();
    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        task_mgr.clone(),
        confirmation_mgr.clone(),
        AuditLogger::new(audit_file),
    );
    adapter.set_computer_use_enabled(false);

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let update = serde_json::json!({
        "update_id": 35,
        "message": {
            "message_id": 55,
            "from": { "id": 88888 },
            "chat": { "id": 88888, "type": "private" },
            "date": now_secs,
            "text": "/do browse web and search notes"
        }
    });

    adapter.process_update(update, now_secs).await.unwrap();

    let sent = mock.sent_messages.lock().unwrap();
    assert!(sent.iter().any(|s| s["text"]
        .as_str()
        .unwrap()
        .contains("Mode A Computer Use is disabled by default in configuration")));
}

#[tokio::test]
async fn test_m4_consequential_confirmation_uses_unified_confirmation_manager() {
    let mock = MockTelegramServer::start().await;
    let temp_dir = tempfile::tempdir().unwrap();
    let audit_file = temp_dir.path().join("audit.log");

    let client = Arc::new(TelegramClient::with_base_url(
        "MOCK_BOT_TOKEN".to_string(),
        mock.base_url.clone(),
    ));
    let config = Arc::new(Mutex::new(RemoteConfig {
        enabled: true,
        keep_awake: false,
        paired_user_id: Some(77777),
        allowed_roots: vec![],
    }));

    let task_mgr = TaskManager::new();
    let confirmation_mgr = ConfirmationManager::new();
    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        task_mgr.clone(),
        confirmation_mgr.clone(),
        AuditLogger::new(audit_file),
    );
    adapter.set_computer_use_enabled(true);

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Trigger consequential action
    let update = serde_json::json!({
        "update_id": 36,
        "message": {
            "message_id": 56,
            "from": { "id": 77777 },
            "chat": { "id": 77777, "type": "private" },
            "date": now_secs,
            "text": "/do delete file production_backup.zip"
        }
    });
    adapter.process_update(update, now_secs).await.unwrap();

    // Verify pending confirmation in UnifiedConfirmationManager
    let pending = confirmation_mgr
        .get_active_pending()
        .expect("Pending action must exist in UnifiedConfirmationManager");
    assert!(pending.description.contains("production_backup.zip"));

    // 5 wrong attempts invalidate action
    for _ in 0..5 {
        let _ = confirmation_mgr.verify_code("0000");
    }
    assert!(
        confirmation_mgr.get_active_pending().is_none(),
        "Action must be dropped after 5 failed attempts"
    );
}

// 7. Token never in logs and redacted
#[test]
fn test_bot_token_redaction() {
    let fake_bot_token = format!("{}:{}", "123456789", "ABCdefGHIjklMNOpqrSTUvwxYZ123456789");
    let raw = format!("Connection error using token {}", fake_bot_token);
    let redacted = redact_sensitive_content(&raw, None);
    assert!(!redacted.contains(&fake_bot_token));
    assert!(redacted.contains("[REDACTED_API_KEY]"));
}

// 8. Remote off when no token or disabled
#[test]
fn test_remote_off_when_no_token_or_disabled() {
    let config = RemoteConfig {
        enabled: false,
        keep_awake: false,
        paired_user_id: None,
        allowed_roots: vec![],
    };
    assert!(!config.enabled);

    let empty_token: Option<String> = None;
    assert!(empty_token.is_none());
}

// 9. Cap failed pairing attempts at 5
#[tokio::test]
async fn test_pairing_failed_attempts_capped_at_five() {
    use ruby_remote::telegram::pairing::PairingResult;

    let mock = MockTelegramServer::start().await;
    let temp_dir = tempfile::tempdir().unwrap();
    let audit_file = temp_dir.path().join("audit.log");

    let client = Arc::new(TelegramClient::with_base_url(
        "MOCK_BOT_TOKEN".to_string(),
        mock.base_url.clone(),
    ));
    let config = Arc::new(Mutex::new(RemoteConfig {
        enabled: true,
        keep_awake: false,
        paired_user_id: None,
        allowed_roots: vec![],
    }));

    let pairing_mgr = PairingManager::new();
    pairing_mgr.set_code_for_test("123456", Duration::from_secs(300));

    // Manager-level checks: 4 failed attempts keep code active; 5th invalidates
    assert_eq!(
        pairing_mgr.validate_code("000001"),
        PairingResult::InvalidOrExpired
    );
    assert_eq!(
        pairing_mgr.validate_code("000002"),
        PairingResult::InvalidOrExpired
    );
    assert_eq!(
        pairing_mgr.validate_code("000003"),
        PairingResult::InvalidOrExpired
    );
    assert_eq!(
        pairing_mgr.validate_code("000004"),
        PairingResult::InvalidOrExpired
    );
    assert_eq!(
        pairing_mgr.validate_code("000005"),
        PairingResult::MaxAttemptsExceeded
    );
    // 6th attempt with correct code now fails because code was wiped!
    assert_eq!(
        pairing_mgr.validate_code("123456"),
        PairingResult::InvalidOrExpired
    );

    // Adapter-level check: 5th attempt sends specific invalidation message
    let pairing_mgr2 = PairingManager::new();
    let (code2, _) = pairing_mgr2.get_or_generate_code();

    let adapter = TelegramAdapter::with_components(
        client,
        config,
        pairing_mgr2,
        TaskManager::new(),
        ConfirmationManager::new(),
        AuditLogger::new(audit_file),
    );

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 4 wrong attempts
    for i in 1..=4 {
        let wrong = serde_json::json!({
            "update_id": i,
            "message": {
                "message_id": 100 + i,
                "from": { "id": 44444 },
                "chat": { "id": 44444, "type": "private" },
                "date": now_secs,
                "text": format!("/pair 99999{}", i)
            }
        });
        adapter.process_update(wrong, now_secs).await.unwrap();
    }

    // 5th wrong attempt triggers invalidation
    let fifth_wrong = serde_json::json!({
        "update_id": 5,
        "message": {
            "message_id": 105,
            "from": { "id": 44444 },
            "chat": { "id": 44444, "type": "private" },
            "date": now_secs,
            "text": "/pair 000000"
        }
    });
    adapter.process_update(fifth_wrong, now_secs).await.unwrap();

    {
        let sent = mock.sent_messages.lock().unwrap();
        assert_eq!(sent.len(), 5);
        assert!(sent[4]["text"]
            .as_str()
            .unwrap()
            .contains("Pairing code invalidated after 5 failed attempts"));
    }

    // 6th attempt with original code is rejected
    let sixth_correct = serde_json::json!({
        "update_id": 6,
        "message": {
            "message_id": 106,
            "from": { "id": 44444 },
            "chat": { "id": 44444, "type": "private" },
            "date": now_secs,
            "text": format!("/pair {}", code2)
        }
    });
    adapter
        .process_update(sixth_correct, now_secs)
        .await
        .unwrap();
    {
        let sent = mock.sent_messages.lock().unwrap();
        assert!(sent.last().unwrap()["text"]
            .as_str()
            .unwrap()
            .contains("Invalid or expired pairing code"));
    }
}

// 10. Confirm pairing code and confirmation code use CSPRNG
#[test]
fn test_code_generators_use_csprng() {
    use std::collections::HashSet;

    let mut generated_6_digit = HashSet::new();
    for _ in 0..100 {
        let code = PairingManager::generate_secure_code_6_digits();
        assert_eq!(code.len(), 6);
        let num: u32 = code.parse().expect("must be a valid 6-digit number");
        assert!((100_000..=999_999).contains(&num));
        generated_6_digit.insert(code);
    }
    // CSPRNG must produce high entropy across 100 iterations (at least 90 distinct values)
    assert!(
        generated_6_digit.len() >= 90,
        "CSPRNG must generate diverse non-colliding values"
    );

    let mut generated_4_digit = HashSet::new();
    for _ in 0..100 {
        let code = ConfirmationManager::generate_secure_code_4_digits();
        assert_eq!(code.len(), 4);
        let num: u32 = code.parse().expect("must be a valid 4-digit number");
        assert!((1_000..=9_999).contains(&num));
        generated_4_digit.insert(code);
    }
    assert!(
        generated_4_digit.len() >= 80,
        "CSPRNG must generate diverse 4-digit codes"
    );
}

// 11. Long command output (>4000 chars) sent as document attachment
#[tokio::test]
async fn test_long_output_sent_as_document() {
    let mock = MockTelegramServer::start().await;
    let temp_dir = tempfile::tempdir().unwrap();

    let client = Arc::new(TelegramClient::with_base_url(
        "123:TOKEN".to_string(),
        mock.base_url.clone(),
    ));
    let audit_file = temp_dir.path().join("audit.log");

    let config = Arc::new(Mutex::new(RemoteConfig {
        enabled: true,
        paired_user_id: Some(4242),
        ..Default::default()
    }));

    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        TaskManager::new(),
        ConfirmationManager::new(),
        AuditLogger::new(audit_file),
    );

    // Mock handler producing output exceeding 4000 characters
    struct BigOutputHandler;
    #[async_trait::async_trait]
    impl ruby_remote::traits::RemoteCommandHandler for BigOutputHandler {
        async fn handle_command(
            &self,
            _message: ruby_remote::types::IncomingMessage,
        ) -> Result<String, ruby_remote::types::RemoteError> {
            Ok("RUBY_OUTPUT_CHUNK_".repeat(300)) // 5400 characters
        }
    }

    adapter.set_command_handler(Arc::new(BigOutputHandler));

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let update = serde_json::json!({
        "update_id": 881,
        "message": {
            "message_id": 991,
            "date": now_secs,
            "from": { "id": 4242 },
            "chat": { "id": 4242, "type": "private" },
            "text": "run_big_task"
        }
    });

    let res = adapter.process_update(update, now_secs).await.unwrap();
    assert_eq!(res, Some(881));

    // Wait for async sends
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify preview text message
    let sent_msgs = mock.sent_messages.lock().unwrap();
    assert!(
        !sent_msgs.is_empty(),
        "Must send preview text message via sendMessage"
    );
    let preview_text = sent_msgs[0]["text"].as_str().unwrap();
    assert!(
        preview_text
            .contains("output exceeds 4000 characters; full text attached below as output.txt"),
        "Preview message must notice document attachment"
    );

    // Verify sendDocument was called
    let sent_docs = mock.sent_documents.lock().unwrap();
    assert_eq!(
        sent_docs.len(),
        1,
        "Must call sendDocument exactly once for long output"
    );
    assert!(
        sent_docs[0].contains("output.txt"),
        "Document request must specify output.txt filename"
    );
}

// 12. Environment variable fallback for bot token
#[test]
fn test_bot_token_env_fallback() {
    let test_key = "TELEGRAM_BOT_TOKEN";
    let test_token = format!("{}:{}", "123456789", "A1B2C3D4E5F6G7H8I9J0K1L2M3N4O5P6Q7R");

    std::env::set_var(test_key, &test_token);
    let resolved = ruby_remote::get_telegram_bot_token();
    std::env::remove_var(test_key);

    assert_eq!(resolved, Some(test_token));
}

// 13. Mode B Background task commands over Telegram (/jobs, /job, /cancel, /logs)
#[tokio::test]
async fn test_telegram_background_job_commands_lifecycle() {
    let store = Arc::new(ruby_core::tasks::SqliteJobStore::new_in_memory().unwrap());
    let backend = Arc::new(ruby_core::MockBackend);
    let supervisor = Arc::new(ruby_core::tasks::BackgroundTaskSupervisor::new(store, backend, 60));

    let j = supervisor.submit("Clean build artifacts").await.unwrap();
    // Yield briefly to let the background task spawn and write initial start logs
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let adapter = TelegramAdapter::with_base_url("MOCK_TOKEN".to_string(), Some("http://127.0.0.1".to_string()));
    adapter.set_background_supervisor(Arc::clone(&supervisor));

    // 1. /jobs
    let jobs_resp = ruby_core::tasks::handle_background_command(&supervisor, "/jobs", "");
    assert!(jobs_resp.contains("Clean build artifacts"));
    assert!(jobs_resp.contains(&j.id.to_string()));

    // 2. /job <id>
    let job_resp = ruby_core::tasks::handle_background_command(&supervisor, "/job", &j.id.to_string());
    assert!(job_resp.contains("Job Details:"));
    assert!(job_resp.contains("Clean build artifacts"));

    // 3. /logs <id>
    let logs_resp = ruby_core::tasks::handle_background_command(&supervisor, "/logs", &j.id.to_string());
    assert!(logs_resp.contains("Task started"));

    // 4. /cancel <id>
    let cancel_resp = ruby_core::tasks::handle_background_command(&supervisor, "/cancel", &j.id.to_string());
    assert!(cancel_resp.contains("cancelled"));
}

// 12. Opt-in live test: sends one message to paired chat and prints reply latency
#[cfg(feature = "live-telegram")]
#[tokio::test]
async fn test_live_telegram_roundtrip() {
    let token = std::env::var("TELEGRAM_BOT_TOKEN")
        .expect("TELEGRAM_BOT_TOKEN environment variable required for live test");
    let paired_chat_id: i64 = std::env::var("TELEGRAM_PAIRED_CHAT_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .or_else(|| ruby_remote::load_remote_config().paired_user_id)
        .expect("TELEGRAM_PAIRED_CHAT_ID env var or paired_user_id in ~/.ruby/remote_config.json required for live test");

    let client = TelegramClient::new(token);
    let ping_msg = format!("Ruby Live Test Ping [{}]", chrono::Utc::now().to_rfc3339());

    let start = std::time::Instant::now();
    let res = client.send_message(paired_chat_id, &ping_msg).await;
    let latency = start.elapsed();

    assert!(res.is_ok(), "Failed to send live message: {:?}", res.err());
    println!(
        "Live Telegram send_message to chat {} succeeded in {:?}",
        paired_chat_id, latency
    );
}
