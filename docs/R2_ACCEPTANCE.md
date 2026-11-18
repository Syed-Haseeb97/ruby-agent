# Phase R2: Remote Screen Capture & Visual Verification — Acceptance Report

**Author**: Ruby Assistant Team  
**Date**: 2026-11-19  
**Repository**: `Syed-Haseeb97/ruby-agent`  
**Milestone**: Phase R2 (Part D)  
**Status**: Completed & Verified  

---

## 1. Overview & Scope

Phase R2 (Part D) delivers remote screen capture and visual observation capabilities over Telegram for paired mobile devices, completing the visual feedback loop for autonomous Mode A and Mode B desktop operations.

In earlier milestones (M4/R0/R1), remote screenshot transmission was deliberately deferred and locked to prevent unintended leakage of visual desktop state. Phase R2 introduces a hardened, policy-gated capture pipeline that strictly enforces session locks, UAC / secure desktop detection, and foreground window security denylists before taking or streaming any desktop pixels.

---

## 2. Key Architecture & Components

### 2.1 RemoteScreenCapturer Trait (`ruby-remote::telegram::screen`)
```rust
#[async_trait]
pub trait RemoteScreenCapturer: Send + Sync {
    async fn capture_screen(&self) -> Result<CapturedScreen, String>;
}

pub struct CapturedScreen {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub mime_type: String,
}
```

### 2.2 DefaultRemoteScreenCapturer (`DefaultRemoteScreenCapturer`)
- Wraps `Arc<dyn ComputerUseExecutor>` and `Arc<dyn PlatformSecurityInspector>` from `ruby-actions`.
- Checks `inspector.is_session_locked().await` before capturing.
- Checks `inspector.is_foreground_elevated_or_secure_desktop().await` before capturing.
- Resolves foreground window title and process name using `ruby_actions::computer_use::get_active_window_info()`.
- Validates active window against `is_window_denylisted_with_custom(&title, &proc, &denylist)`: built-in password managers (1Password, Bitwarden, KeePass), financial portals (Chase, PayPal), system administration (`regedit`, `taskmgr`), and user-configured custom denylists.
- Streams in-memory JPEG bytes directly into `CapturedScreen` without ever persisting raw screen images to disk in the repo tree.

### 2.3 Native Photo Endpoint & Fallback (`TelegramClient::send_photo`)
- Calls Telegram Bot API endpoint `/sendPhoto` with `multipart/form-data` containing `photo` byte stream and contextual caption `📸 Desktop Screenshot ({width}x{height})`.
- If `/sendPhoto` encounters API limitations (e.g. dimensions or mime requirements), `TelegramAdapter` automatically falls back to `send_document` as `screen.jpg`.
- `ChannelAdapter::send_file` intelligently routes image filenames (`.jpg`, `.jpeg`, `.png`) to `send_photo` and non-image files to `send_document`.

### 2.4 Telegram `/screen` Command & Audit Trail
- Available to paired users in private Telegram chats.
- Silently ignores unpaired or unauthorized senders.
- Rejects stale messages (>120s old).
- Records all actions to append-only security audit log (`remote_audit.log`):
  - `[TIMESTAMP] sender=12345 task=- cmd=screen outcome=success`
  - `[TIMESTAMP] sender=12345 task=- cmd=screen outcome=refused_windows_session_is_locked`
  - `[TIMESTAMP] sender=12345 task=- cmd=screen outcome=refused_secure_desktop_or_elevated_process_active`
  - `[TIMESTAMP] sender=12345 task=- cmd=screen outcome=refused_security_denylist`

---

## 3. Privacy & Security Invariants

1. **Rule S1 Compliance (Private Data Out of Repo)**:
   - No screenshot files are saved to disk in the workspace or git repository.
   - Screen capture data lives strictly in transient heap memory (`Vec<u8>`) during transmission to the Telegram Bot API over HTTPS.
2. **Rule G3 Compliance (Negative Invariant)**:
   - Automated unit test suites execute entirely hermetically using `MockRemoteScreenCapturer` or `MockComputerUseExecutor` and `MockPlatformSecurityInspector`.
   - Zero physical hardware captures or screen reading occurred during automated testing.
3. **Rule G4 Compliance (Dictation Isolation)**:
   - Zero edits made to `keyboard_hook.rs`, `text_injection.rs`, `elevation_check.rs`, or `dictation_engine.rs`.

---

## 4. Phase R2 Verification Matrix

| ID | Item | Scope | Test / Verification Method | Verification Status |
|:---|:---|:---|:---|:---|
| **R2-1** | **`/screen` Command Success** | Dispatches `/screen` from paired chat, verifies `sendPhoto` receives JPEG data with correct dimensions and audit log records success. | `test_telegram_screen_command_success` in `mock_telegram_tests.rs` | `mock-level` |
| **R2-2** | **Session Locked Refusal** | Refuses capture when Windows session is locked, returns security notice to user, 0 photos sent. | `test_telegram_screen_command_session_locked_refusal` in `mock_telegram_tests.rs` | `mock-level` |
| **R2-3** | **Elevated / Secure Desktop Refusal** | Refuses capture when UAC secure desktop or elevated window is active, 0 photos sent. | `test_telegram_screen_command_elevated_or_secure_desktop_refusal` in `mock_telegram_tests.rs` | `mock-level` |
| **R2-4** | **Security Denylist Refusal** | Refuses capture when foreground window matches password managers or banking portals. | `test_telegram_screen_command_denylist_refusal` in `mock_telegram_tests.rs` | `mock-level` |
| **R2-5** | **Uninitialized State Safety** | Returns clean explanatory error when screen capturer is uninitialized or computer use unavailable. | `test_telegram_screen_command_uninitialized` in `mock_telegram_tests.rs` | `mock-level` |
| **R2-6** | **DefaultRemoteScreenCapturer Guardrails** | Hermetic test of `DefaultRemoteScreenCapturer` verifying built-in denylist, custom user denylist, session locks, and elevated window gating. | `test_default_remote_screen_capturer_denylist_and_locks` in `mock_telegram_tests.rs` | `mock-level` |
| **R2-7** | **send_file Transport Routing** | Verifies `ChannelAdapter::send_file` dispatches `.png`/`.jpg` via `sendPhoto` and `.pdf`/other docs via `sendDocument`. | `test_channel_adapter_send_file_routes_images_to_send_photo` in `mock_telegram_tests.rs` | `mock-level` |
| **R2-8** | **Desktop State Wiring** | Initializes `WindowsComputerUseExecutor`, `WindowsPlatformSecurityInspector`, and custom denylist into `TelegramAdapter` in `ruby-desktop`. | `crates/ruby-desktop/src/lib.rs` (compiles and passes `cargo check -p ruby-desktop`) | `compiles-only` |
| **R2-9** | **Live Physical Capture** | Real Win32 GDI screen capture and live Telegram delivery to physical mobile device. | Opt-in via `--features live-telegram` and physical desktop run; documented in `R2_MANUAL_TESTING.md`. | `UNVERIFIED` (Requires Haseeb's live mobile device) |
