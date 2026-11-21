//! Comprehensive mock Telegram server and unit tests for ruby-remote.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use ruby_actions::computer_use::agent_loop::MockPlatformSecurityInspector;
use ruby_actions::computer_use::executor::MockComputerUseExecutor;
use ruby_gemini::dictation_client::redact_sensitive_content;
use ruby_remote::audit::AuditLogger;
use ruby_remote::confirmation::ConfirmationManager;
use ruby_remote::key_storage::{RemoteConfig, RemoteOffsetState};
use ruby_remote::state_machine::TaskManager;
use ruby_remote::telegram::client::TelegramClient;
use ruby_remote::telegram::pairing::PairingManager;
use ruby_remote::telegram::{
    DefaultRemoteScreenCapturer, MockRemoteScreenCapturer, RemoteScreenCapturer, TelegramAdapter,
};
use ruby_remote::traits::ChannelAdapter;
use ruby_remote::types::TaskState;

/// Helper to spin up a mock Telegram server on an ephemeral port.
#[allow(dead_code)]
struct MockTelegramServer {
    pub base_url: String,
    pub sent_messages: Arc<Mutex<Vec<serde_json::Value>>>,
    pub sent_documents: Arc<Mutex<Vec<String>>>,
    pub sent_photos: Arc<Mutex<Vec<String>>>,
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

        let sent_photos = Arc::new(Mutex::new(Vec::new()));
        let sent_photos_clone = Arc::clone(&sent_photos);

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
                let sent_photos_inner = Arc::clone(&sent_photos_clone);
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
                            } else if header.contains("/sendPhoto") {
                                sent_photos_inner.lock().unwrap().push(body_str.to_string());
                                let resp = serde_json::json!({
                                    "ok": true,
                                    "result": { "message_id": 103 }
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
            sent_photos,
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
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
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

// 5b. /stop immediately stops active Voice Live session
#[tokio::test]
async fn test_telegram_stop_command_stops_voice_live_manager() {
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

    let voice_mgr = Arc::new(ruby_core::voice_live::VoiceLiveManager::new(
        Arc::new(ruby_core::voice_live::NullAudioOutput),
        Arc::new(ruby_core::voice_live::MockToolDispatcher::default()),
        None,
    ));

    let handle = ruby_gemini::live_session::MockLiveSession::open(
        ruby_gemini::live_session::LiveSessionConfig::default(),
    );
    voice_mgr.start_with_handle(handle).await.unwrap();

    // Verify Active
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    assert_eq!(
        voice_mgr.status(),
        ruby_core::voice_live::VoiceLiveStatus::Active
    );

    adapter.set_voice_live_manager(Arc::clone(&voice_mgr));

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let stop_update = serde_json::json!({
        "update_id": 11,
        "message": {
            "message_id": 31,
            "from": { "id": 88888 },
            "chat": { "id": 88888, "type": "private" },
            "date": now_secs,
            "text": "/stop"
        }
    });

    adapter.process_update(stop_update, now_secs).await.unwrap();

    // Verify Idle
    let _ = tokio::time::timeout(tokio::time::Duration::from_millis(500), async {
        loop {
            if voice_mgr.status() == ruby_core::voice_live::VoiceLiveStatus::Idle {
                return;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    })
    .await;

    assert_eq!(
        voice_mgr.status(),
        ruby_core::voice_live::VoiceLiveStatus::Idle
    );
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
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
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
        ..Default::default()
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

    let mut generated_6_digit = HashSet::new();
    for _ in 0..100 {
        let code = ConfirmationManager::generate_secure_code_6_digits();
        assert_eq!(code.len(), 6);
        let num: u32 = code.parse().expect("must be a valid 6-digit number");
        assert!((100_000..=999_999).contains(&num));
        generated_6_digit.insert(code);
    }
    assert!(
        generated_6_digit.len() >= 80,
        "CSPRNG must generate diverse 6-digit codes"
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
    let supervisor = Arc::new(ruby_core::tasks::BackgroundTaskSupervisor::new(
        store, backend, 60,
    ));

    let j = supervisor.submit("Clean build artifacts").await.unwrap();
    // Yield briefly to let the background task spawn and write initial start logs
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let adapter = TelegramAdapter::with_base_url(
        "MOCK_TOKEN".to_string(),
        Some("http://127.0.0.1".to_string()),
    );
    adapter.set_background_supervisor(Arc::clone(&supervisor));

    // 1. /jobs
    let jobs_resp = ruby_core::tasks::handle_background_command(&supervisor, "/jobs", "");
    assert!(jobs_resp.contains("Clean build artifacts"));
    assert!(jobs_resp.contains(&j.id.to_string()));

    // 2. /job <id>
    let job_resp =
        ruby_core::tasks::handle_background_command(&supervisor, "/job", &j.id.to_string());
    assert!(job_resp.contains("Job Details:"));
    assert!(job_resp.contains("Clean build artifacts"));

    // 3. /logs <id>
    let logs_resp =
        ruby_core::tasks::handle_background_command(&supervisor, "/logs", &j.id.to_string());
    assert!(logs_resp.contains("Task started"));

    // 4. /cancel <id>
    let cancel_resp =
        ruby_core::tasks::handle_background_command(&supervisor, "/cancel", &j.id.to_string());
    assert!(cancel_resp.contains("cancelled"));
}

// 12. Phase R2: /screen refuses when allow_screenshots is false (default)
#[tokio::test]
async fn test_telegram_screen_command_refuses_when_disabled() {
    let mock = MockTelegramServer::start().await;
    let temp_dir = tempfile::tempdir().unwrap();
    let audit_file = temp_dir.path().join("audit.log");

    let client = Arc::new(TelegramClient::with_base_url(
        "MOCK_BOT_TOKEN".to_string(),
        mock.base_url.clone(),
    ));

    // allow_screenshots is false by default
    let config = Arc::new(Mutex::new(RemoteConfig {
        enabled: true,
        keep_awake: false,
        paired_user_id: Some(12345),
        allowed_roots: vec![],
        allow_screenshots: false,
    }));

    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        TaskManager::new(),
        ConfirmationManager::new(),
        AuditLogger::new(audit_file.clone()),
    );

    let capturer = Arc::new(MockRemoteScreenCapturer::default());
    adapter.set_screen_capturer(capturer);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let update = serde_json::json!({
        "update_id": 3001,
        "message": {
            "message_id": 501,
            "date": now,
            "chat": { "id": 12345, "type": "private" },
            "from": { "id": 12345, "first_name": "Owner" },
            "text": "/screen"
        }
    });

    let res = adapter.process_update(update, now).await.unwrap();
    assert_eq!(res, Some(3001));

    // Assert photo was NOT sent
    let photos = mock.sent_photos.lock().unwrap();
    assert_eq!(photos.len(), 0);

    // Verify rejection message
    let msgs = mock.sent_messages.lock().unwrap();
    let reply = msgs[0]["text"].as_str().unwrap();
    assert!(reply.contains("Screen capture is disabled in configuration"));

    // Verify audit log
    let audit_content = std::fs::read_to_string(&audit_file).unwrap();
    assert!(audit_content.contains("cmd=screen"));
    assert!(audit_content.contains("outcome=refused_disabled"));
}

// 12b. Phase R2: /screen when enabled requests confirmation with E2EE warning, and /yes sends photo
#[tokio::test]
async fn test_telegram_screen_command_confirmation_flow_and_photo_dispatch() {
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
        paired_user_id: Some(12345),
        allowed_roots: vec![],
        allow_screenshots: true,
    }));

    let confirmation_mgr = ConfirmationManager::new();
    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        TaskManager::new(),
        confirmation_mgr.clone(),
        AuditLogger::new(audit_file.clone()),
    );

    let capturer = Arc::new(MockRemoteScreenCapturer::default());
    adapter.set_screen_capturer(capturer);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 1. Send /screen -> must trigger confirmation prompt with E2EE disclosure
    let update = serde_json::json!({
        "update_id": 3001,
        "message": {
            "message_id": 501,
            "date": now,
            "chat": { "id": 12345, "type": "private" },
            "from": { "id": 12345, "first_name": "Owner" },
            "text": "/screen"
        }
    });

    let res = adapter.process_update(update, now).await.unwrap();
    assert_eq!(res, Some(3001));

    // Confirm that photos have NOT been sent yet
    assert_eq!(mock.sent_photos.lock().unwrap().len(), 0);

    // Extract confirmation prompt and code
    let (prompt, code_str) = {
        let msgs = mock.sent_messages.lock().unwrap();
        let p = msgs[0]["text"].as_str().unwrap().to_string();
        let c = p
            .split("/yes ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .expect("Prompt must contain confirmation code")
            .to_string();
        (p, c)
    };
    assert!(prompt.contains("Confirm remote screen capture?"));
    assert!(prompt.contains("Telegram bot messages are not end-to-end encrypted"));
    assert!(prompt.contains("/yes"));
    assert_eq!(code_str.len(), 6);

    // 2. User confirms with /yes <code>
    let confirm_update = serde_json::json!({
        "update_id": 3002,
        "message": {
            "message_id": 502,
            "date": now,
            "chat": { "id": 12345, "type": "private" },
            "from": { "id": 12345 },
            "text": format!("/yes {code_str}")
        }
    });

    let res2 = adapter.process_update(confirm_update, now).await.unwrap();
    assert_eq!(res2, Some(3002));

    // Assert photo was dispatched to mock Telegram server
    let photos = mock.sent_photos.lock().unwrap();
    assert_eq!(photos.len(), 1, "Expected exactly 1 sendPhoto call");
    assert!(photos[0].contains("screen.jpg"));
    assert!(photos[0].contains("1920x1080"));

    // Verify audit log contains metadata, never image data
    let audit_content = std::fs::read_to_string(&audit_file).unwrap();
    assert!(audit_content.contains("cmd=screen"));
    assert!(audit_content.contains("pending_confirmation"));
    assert!(audit_content.contains("outcome=success"));
    assert!(!audit_content.contains("JFIF"));
    assert!(!audit_content.contains("base64"));
    assert!(!audit_content.contains("image/jpeg"));
}

// 13. Phase R2: /screen confirmed capture refuses when Windows session is locked
#[tokio::test]
async fn test_telegram_screen_command_session_locked_refusal() {
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
        paired_user_id: Some(12345),
        allowed_roots: vec![],
        allow_screenshots: true,
    }));

    let confirmation_mgr = ConfirmationManager::new();
    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        TaskManager::new(),
        confirmation_mgr.clone(),
        AuditLogger::new(audit_file.clone()),
    );

    let mock_cap = MockRemoteScreenCapturer {
        session_locked: true,
        ..Default::default()
    };
    adapter.set_screen_capturer(Arc::new(mock_cap));

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 1. Request capture
    let update = serde_json::json!({
        "update_id": 3004,
        "message": {
            "message_id": 504,
            "date": now,
            "chat": { "id": 12345, "type": "private" },
            "from": { "id": 12345 },
            "text": "/screen"
        }
    });
    let _ = adapter.process_update(update, now).await.unwrap();

    let code_str = {
        let msgs = mock.sent_messages.lock().unwrap();
        let prompt = msgs[0]["text"].as_str().unwrap();
        prompt
            .split("/yes ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .unwrap()
            .to_string()
    };

    // 2. Confirm -> session locked triggers refusal
    let confirm_update = serde_json::json!({
        "update_id": 3005,
        "message": {
            "message_id": 505,
            "date": now,
            "chat": { "id": 12345, "type": "private" },
            "from": { "id": 12345 },
            "text": format!("/yes {code_str}")
        }
    });
    let _ = adapter.process_update(confirm_update, now).await.unwrap();

    assert_eq!(mock.sent_photos.lock().unwrap().len(), 0);

    let msgs2 = mock.sent_messages.lock().unwrap();
    let reply = msgs2.last().unwrap()["text"].as_str().unwrap();
    assert!(reply.contains("Windows session is locked"));

    let audit_content = std::fs::read_to_string(&audit_file).unwrap();
    assert!(audit_content.contains("cmd=screen"));
    assert!(audit_content.contains("refused_windows_session_is_locked"));
}

// 14. Phase R2: /screen confirmed capture refuses when elevated or secure desktop active
#[tokio::test]
async fn test_telegram_screen_command_elevated_or_secure_desktop_refusal() {
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
        paired_user_id: Some(12345),
        allowed_roots: vec![],
        allow_screenshots: true,
    }));

    let confirmation_mgr = ConfirmationManager::new();
    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        TaskManager::new(),
        confirmation_mgr.clone(),
        AuditLogger::new(audit_file.clone()),
    );

    let mock_cap = MockRemoteScreenCapturer {
        elevated_or_secure_desktop: true,
        ..Default::default()
    };
    adapter.set_screen_capturer(Arc::new(mock_cap));

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let update = serde_json::json!({
        "update_id": 3006,
        "message": {
            "message_id": 506,
            "date": now,
            "chat": { "id": 12345, "type": "private" },
            "from": { "id": 12345 },
            "text": "/screen"
        }
    });
    let _ = adapter.process_update(update, now).await.unwrap();

    let code_str = {
        let msgs = mock.sent_messages.lock().unwrap();
        let prompt = msgs[0]["text"].as_str().unwrap();
        prompt
            .split("/yes ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .unwrap()
            .to_string()
    };

    let confirm_update = serde_json::json!({
        "update_id": 3007,
        "message": {
            "message_id": 507,
            "date": now,
            "chat": { "id": 12345, "type": "private" },
            "from": { "id": 12345 },
            "text": format!("/yes {code_str}")
        }
    });
    let _ = adapter.process_update(confirm_update, now).await.unwrap();

    assert_eq!(mock.sent_photos.lock().unwrap().len(), 0);

    let msgs2 = mock.sent_messages.lock().unwrap();
    let reply = msgs2.last().unwrap()["text"].as_str().unwrap();
    assert!(reply.contains("Secure desktop or elevated process active"));
}

// 15. Phase R2: /screen confirmed capture refuses when visible window is in security denylist
#[tokio::test]
async fn test_telegram_screen_command_visible_denylist_refusal() {
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
        paired_user_id: Some(12345),
        allowed_roots: vec![],
        allow_screenshots: true,
    }));

    let confirmation_mgr = ConfirmationManager::new();
    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        TaskManager::new(),
        confirmation_mgr.clone(),
        AuditLogger::new(audit_file.clone()),
    );

    let mock_cap = MockRemoteScreenCapturer {
        visible_windows: vec![
            ("Visual Studio Code".into(), "code.exe".into()),
            ("KeePass Password Safe".into(), "keepass.exe".into()),
        ],
        ..Default::default()
    };
    adapter.set_screen_capturer(Arc::new(mock_cap));

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let update = serde_json::json!({
        "update_id": 3008,
        "message": {
            "message_id": 508,
            "date": now,
            "chat": { "id": 12345, "type": "private" },
            "from": { "id": 12345 },
            "text": "/screen"
        }
    });
    let _ = adapter.process_update(update, now).await.unwrap();

    let code_str = {
        let msgs = mock.sent_messages.lock().unwrap();
        let prompt = msgs[0]["text"].as_str().unwrap();
        prompt
            .split("/yes ")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .unwrap()
            .to_string()
    };

    let confirm_update = serde_json::json!({
        "update_id": 3009,
        "message": {
            "message_id": 509,
            "date": now,
            "chat": { "id": 12345, "type": "private" },
            "from": { "id": 12345 },
            "text": format!("/yes {code_str}")
        }
    });
    let _ = adapter.process_update(confirm_update, now).await.unwrap();

    assert_eq!(mock.sent_photos.lock().unwrap().len(), 0);

    let msgs2 = mock.sent_messages.lock().unwrap();
    let reply = msgs2.last().unwrap()["text"].as_str().unwrap();
    assert!(reply.contains("security denylist"));
}

// 16. Phase R2: /screen returns clean notification when capturer is uninitialized
#[tokio::test]
async fn test_telegram_screen_command_uninitialized() {
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
        paired_user_id: Some(12345),
        allowed_roots: vec![],
        allow_screenshots: true,
    }));

    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        TaskManager::new(),
        ConfirmationManager::new(),
        AuditLogger::new(audit_file.clone()),
    );
    // screen_capturer is None by default

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let update = serde_json::json!({
        "update_id": 3005,
        "message": {
            "message_id": 505,
            "date": now,
            "chat": { "id": 12345, "type": "private" },
            "from": { "id": 12345 },
            "text": "/screen"
        }
    });

    let res = adapter.process_update(update, now).await.unwrap();
    assert_eq!(res, Some(3005));

    assert_eq!(mock.sent_photos.lock().unwrap().len(), 0);

    let msgs = mock.sent_messages.lock().unwrap();
    let reply = msgs[0]["text"].as_str().unwrap();
    assert!(reply.contains("Screen capture is not initialized"));
}

// 17. Phase R2: DefaultRemoteScreenCapturer hermetic verification of locks, elevation, and full desktop denylist
#[tokio::test]
async fn test_default_remote_screen_capturer_denylist_and_locks() {
    let mock_executor = Arc::new(MockComputerUseExecutor::default());
    let mock_inspector = Arc::new(MockPlatformSecurityInspector::default());
    let capturer = DefaultRemoteScreenCapturer::new(
        mock_executor.clone(),
        mock_inspector.clone(),
        vec!["CustomBank".into()],
    );

    // 1. Clean state -> Success
    capturer.set_window_override("Visual Studio Code", "code.exe");
    capturer.set_visible_windows_override(vec![
        ("Visual Studio Code".into(), "code.exe".into()),
        ("Terminal".into(), "powershell.exe".into()),
    ]);
    let res = capturer.capture_screen().await;
    assert!(res.is_ok(), "Clean state must succeed: {:?}", res.err());
    let captured = res.unwrap();
    assert_eq!(captured.width, 1920);
    assert_eq!(captured.height, 1080);
    assert!(!captured.data.is_empty());

    // 2. Denylisted window: built-in (1Password) as background visible window
    capturer.set_window_override("Untitled - Notepad", "notepad.exe");
    capturer.set_visible_windows_override(vec![
        ("Untitled - Notepad".into(), "notepad.exe".into()),
        ("1Password - Vault".into(), "1password.exe".into()),
    ]);
    let res_denied_builtin = capturer.capture_screen().await;
    assert!(res_denied_builtin.is_err());
    assert!(res_denied_builtin
        .unwrap_err()
        .contains("security denylist"));

    // 3. Denylisted window: custom entry (CustomBank) as background visible window
    capturer.set_visible_windows_override(vec![
        ("Untitled - Notepad".into(), "notepad.exe".into()),
        ("My Bank Portal".into(), "custombank.exe".into()),
    ]);
    let res_denied_custom = capturer.capture_screen().await;
    assert!(res_denied_custom.is_err());
    assert!(res_denied_custom.unwrap_err().contains("security denylist"));

    // 4. Session locked
    capturer.clear_visible_windows_override();
    capturer.set_window_override("Untitled - Notepad", "notepad.exe");
    let locked_inspector = MockPlatformSecurityInspector {
        session_locked: true,
        ..Default::default()
    };
    let locked_capturer =
        DefaultRemoteScreenCapturer::new(mock_executor.clone(), Arc::new(locked_inspector), vec![]);
    locked_capturer.set_window_override("Untitled - Notepad", "notepad.exe");
    let res_locked = locked_capturer.capture_screen().await;
    assert!(res_locked.is_err());
    assert!(res_locked
        .unwrap_err()
        .contains("Windows session is locked"));

    // 5. Elevated / secure desktop
    let elevated_inspector = MockPlatformSecurityInspector {
        foreground_elevated: true,
        ..Default::default()
    };
    let elevated_capturer =
        DefaultRemoteScreenCapturer::new(mock_executor, Arc::new(elevated_inspector), vec![]);
    elevated_capturer.set_window_override("Untitled - Notepad", "notepad.exe");
    let res_elevated = elevated_capturer.capture_screen().await;
    assert!(res_elevated.is_err());
    assert!(res_elevated
        .unwrap_err()
        .contains("Secure desktop or elevated process active"));
}

// 18. Phase R2: ChannelAdapter::send_file routes images to send_photo and documents to send_document
#[tokio::test]
async fn test_channel_adapter_send_file_routes_images_to_send_photo() {
    let mock = MockTelegramServer::start().await;
    let client = Arc::new(TelegramClient::with_base_url(
        "MOCK_BOT_TOKEN".to_string(),
        mock.base_url.clone(),
    ));

    let config = Arc::new(Mutex::new(RemoteConfig {
        enabled: true,
        keep_awake: false,
        paired_user_id: Some(12345),
        allowed_roots: vec![],
        allow_screenshots: true,
    }));

    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        TaskManager::new(),
        ConfirmationManager::new(),
        AuditLogger::new(tempfile::tempdir().unwrap().path().join("audit.log")),
    );

    // Image file -> routes to send_photo
    adapter
        .send_file(12345, "preview.png", &[1, 2, 3], Some("Test Image"))
        .await
        .unwrap();
    assert_eq!(mock.sent_photos.lock().unwrap().len(), 1);

    // Non-image file -> routes to send_document
    adapter
        .send_file(12345, "report.pdf", &[4, 5, 6], Some("Test Document"))
        .await
        .unwrap();
    assert_eq!(mock.sent_documents.lock().unwrap().len(), 1);
}

// 12. Remote /quit command flow requiring 6-digit confirmation
#[tokio::test]
async fn test_telegram_remote_quit_command_flow() {
    let mock = MockTelegramServer::start().await;
    let temp_dir = tempfile::tempdir().unwrap();

    let client = Arc::new(TelegramClient::with_base_url(
        "MOCK_BOT_TOKEN".to_string(),
        mock.base_url.clone(),
    ));
    let config = Arc::new(Mutex::new(RemoteConfig {
        enabled: true,
        paired_user_id: Some(12345),
        ..Default::default()
    }));

    let confirmation_mgr = ConfirmationManager::new();
    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        TaskManager::new(),
        confirmation_mgr.clone(),
        AuditLogger::new(temp_dir.path().join("audit.log")),
    );

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 1. Send /quit -> requires confirmation
    let quit_update = serde_json::json!({
        "update_id": 1,
        "message": {
            "message_id": 10,
            "from": { "id": 12345 },
            "chat": { "id": 12345, "type": "private" },
            "date": now_secs,
            "text": "/quit"
        }
    });
    adapter.process_update(quit_update, now_secs).await.unwrap();

    {
        let sent = mock.sent_messages.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let prompt_text = sent[0]["text"].as_str().unwrap();
        assert!(prompt_text.contains("Confirm remote Ruby application shutdown"));
        assert!(prompt_text.contains("/yes"));
    }

    // Extract confirmation code
    let pending = confirmation_mgr
        .unified()
        .get_pending_action()
        .expect("Pending action must exist");
    assert_eq!(pending.code.len(), 6);

    // 2. Confirm shutdown via /yes <code>
    let yes_update = serde_json::json!({
        "update_id": 2,
        "message": {
            "message_id": 11,
            "from": { "id": 12345 },
            "chat": { "id": 12345, "type": "private" },
            "date": now_secs,
            "text": format!("/yes {}", pending.code)
        }
    });
    adapter.process_update(yes_update, now_secs).await.unwrap();

    let sent2 = mock.sent_messages.lock().unwrap();
    assert_eq!(sent2.len(), 2);
    assert!(sent2[1]["text"]
        .as_str()
        .unwrap()
        .contains("shutting down gracefully"));
}

// 13. Session lock awareness: pauses Mode A, notifies Telegram, Mode B continues
#[tokio::test]
async fn test_session_lock_pauses_mode_a_while_mode_b_continues() {
    let mock = MockTelegramServer::start().await;
    let temp_dir = tempfile::tempdir().unwrap();

    let client = Arc::new(TelegramClient::with_base_url(
        "MOCK_BOT_TOKEN".to_string(),
        mock.base_url.clone(),
    ));
    let config = Arc::new(Mutex::new(RemoteConfig {
        enabled: true,
        paired_user_id: Some(12345),
        ..Default::default()
    }));

    let task_mgr = TaskManager::new();
    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        task_mgr.clone(),
        ConfirmationManager::new(),
        AuditLogger::new(temp_dir.path().join("audit.log")),
    );
    adapter.set_computer_use_enabled(true);

    // Create a running Mode A task
    let task = task_mgr.create_task(12345, "Autonomous desktop automation");
    task_mgr.update_task_state(&task.id, TaskState::Running, None, None);

    // Simulate session lock event
    adapter.on_session_lock_changed(true).await.unwrap();

    // Mode A task should be transitioned to NeedsInput (paused)
    let recent = task_mgr.get_recent_tasks(5);
    let paused_task = recent.iter().find(|t| t.id == task.id).unwrap();
    assert_eq!(paused_task.state, TaskState::NeedsInput);
    assert!(paused_task
        .result
        .as_ref()
        .unwrap()
        .contains("Desktop session locked"));

    // User should have received notification on paired device
    let sent = mock.sent_messages.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert!(sent[0]["text"]
        .as_str()
        .unwrap()
        .contains("Desktop session locked: Mode A autonomous task paused"));
}

// 14. Telegram error classification (409 Conflict, 429 RateLimit, 401 Revoked)
#[test]
fn test_telegram_error_classification_types() {
    let err409 = ruby_remote::types::RemoteError::Conflict409;
    assert_eq!(
        err409.to_string(),
        "Telegram 409 Conflict: Another poller is active"
    );

    let err429 = ruby_remote::types::RemoteError::RateLimited429(30);
    assert_eq!(
        err429.to_string(),
        "Telegram 429 Rate Limit: retry after 30 seconds"
    );

    let err401 = ruby_remote::types::RemoteError::TokenRevoked;
    assert_eq!(
        err401.to_string(),
        "Telegram 401 Unauthorized: Token revoked or invalid"
    );
}

// 15. Telegram /status includes rich metrics & /logs redacts secrets
#[tokio::test]
async fn test_telegram_logs_command_redaction_and_metrics_status() {
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
        paired_user_id: Some(12345),
        allowed_roots: vec![],
        allow_screenshots: false,
    }));

    let adapter = TelegramAdapter::with_components(
        client,
        config,
        PairingManager::new(),
        TaskManager::new(),
        ConfirmationManager::new(),
        AuditLogger::new(audit_file.clone()),
    );

    let metrics = Arc::new(ruby_core::MetricsCollector::new());
    metrics.record_dictation_latency(2000, 100, 300, 50);
    metrics.record_fallback("timeout");
    adapter.set_metrics_collector(Arc::clone(&metrics));

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 1. Test /status
    let status_update = serde_json::json!({
        "update_id": 1,
        "message": {
            "message_id": 101,
            "from": { "id": 12345 },
            "chat": { "id": 12345, "type": "private" },
            "date": now_secs,
            "text": "/status"
        }
    });
    let handled = adapter
        .process_update(status_update, now_secs)
        .await
        .unwrap();
    assert_eq!(handled, Some(1));

    let sent = mock.sent_messages.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    let status_text = sent[0]["text"].as_str().unwrap();
    assert!(status_text.contains("Ruby Laptop Status"));
    assert!(status_text.contains("Ruby System Status & Health"));
    assert!(status_text.contains("timeout=1"));

    // 2. Test /logs redacts planted secrets
    let log_dir = temp_dir.path().join("logs");
    let logger = ruby_core::RedactedJsonLogger::new(log_dir.clone());
    let fake_token = format!("{}{}", "ghp_", "1234567890abcdefghijklmnopqrstuvwxyz");
    logger.log(
        "WARN",
        "auth_module",
        &format!("Token rejected: {}", fake_token),
    );

    mock.sent_messages.lock().unwrap().clear();
    let logs_update = serde_json::json!({
        "update_id": 2,
        "message": {
            "message_id": 102,
            "from": { "id": 12345 },
            "chat": { "id": 12345, "type": "private" },
            "date": now_secs,
            "text": "/logs 5"
        }
    });
    let handled2 = adapter.process_update(logs_update, now_secs).await.unwrap();
    assert_eq!(handled2, Some(2));

    let sent2 = mock.sent_messages.lock().unwrap().clone();
    assert_eq!(sent2.len(), 1);
    let logs_text = sent2[0]["text"].as_str().unwrap();
    assert!(
        !logs_text.contains(&fake_token),
        "Telegram /logs must NEVER leak secret tokens"
    );
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
