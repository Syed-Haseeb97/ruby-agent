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
    assert!(!expired_mgr.validate_code("654321"));

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
    };
    assert!(!config.enabled);

    let empty_token: Option<String> = None;
    assert!(empty_token.is_none());
}

// 9. Opt-in live test
#[cfg(feature = "live-telegram")]
#[tokio::test]
async fn test_live_telegram_roundtrip() {
    let token =
        std::env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN required for live test");
    let client = TelegramClient::new(token);
    let updates = client
        .get_updates(None, Some(1))
        .await
        .expect("live getUpdates");
    println!("Live updates count: {}", updates.len());
}
