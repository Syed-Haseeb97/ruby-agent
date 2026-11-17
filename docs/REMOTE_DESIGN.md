# Remote Control Architecture & Specification (Phase R0)

**Component**: `ruby-remote`  
**Milestone**: Phase R0 (Telegram Channel for Ruby Assistant)  
**Status**: Approved Architecture & Specification  
**Residency**: Strictly Drive D (`D:\projects\ruby-agent`)  

---

## 1. Overview & Goals

Phase R0 establishes the foundation for bidirectional remote control of the Ruby Assistant desktop instance from mobile devices (e.g. smartphone Telegram app). 

### Primary Goal
A user messages their private Telegram bot from their phone; Ruby running on their laptop receives the message, processes administrative and task commands, tracks execution state through a clean task engine API, and sends structured replies.

### Boundaries & Phasing
- **Phase R0 (This Phase)**: Executes **NO** tools, shell commands, or arbitrary file access. It receives commands, handles pairing, enforces security and staleness policies, manages the task state machine, provides status/power metrics, confirmation plumbing, and replies.
- **Phase R1 (Future)**: Plugs tool execution (`ruby-policy`, sandboxed execution, approval escalation, streaming output) into the R0 Task Engine API.
- **Phase R2 (Future)**: Plugs GUI/screenshot capture (`ruby-platform`, desktop capture API, visual observation) into the R0 reply/file pipeline.

---

## 2. Crate Layout & Architecture

The remote system is packaged as an independent workspace crate: `crates/ruby-remote`.

```
crates/ruby-remote/
├── Cargo.toml
├── src/
│   ├── lib.rs                  # Module exports and crate root
│   ├── traits.rs               # ChannelAdapter trait & RemoteCommandHandler
│   ├── types.rs                # Task, State, Command, Message types
│   ├── state_machine.rs        # Task state machine & TaskManager
│   ├── confirmation.rs         # 4-digit confirmation action plumbing (120s TTL)
│   ├── audit.rs                # Append-only local security audit logger
│   ├── power.rs                # Win32 GetSystemPowerStatus & uptime reporter
│   ├── keep_awake.rs           # SetThreadExecutionState AC keep-awake wrapper
│   ├── key_storage.rs          # Keyring & env retrieval for TELEGRAM_BOT_TOKEN
│   └── telegram/
│       ├── mod.rs              # Telegram adapter implementation
│       ├── client.rs           # Long-polling client (getUpdates & sendMessage)
│       ├── pairing.rs          # 5-minute ephemeral pairing logic
│       └── rate_limiter.rs     # Rate limiting for unknown senders
└── tests/
    └── mock_telegram_tests.rs  # Mock server unit & integration tests
```

### Workspace Dependency Graph
- `ruby-remote` depends on `ruby-core` for shared primitives (`OperationalState`, `OrbVisualState`).
- `ruby-desktop` depends on `ruby-remote` to start polling when configured and render remote status in the Console.
- `ruby-remote` does NOT depend on `ruby-dictation`, `ruby-voice`, or `ruby-platform`'s keyboard hooks, preserving strict isolation.

---

## 3. ChannelAdapter Trait (Multi-Platform Extensibility)

To ensure future channels (such as Discord, Slack, or WebSockets) plug in seamlessly, all remote adapters implement the asynchronous `ChannelAdapter` trait:

```rust
use async_trait::async_trait;
use std::sync::Arc;
use crate::types::{IncomingMessage, RemoteError};

#[async_trait]
pub trait RemoteCommandHandler: Send + Sync {
    async fn handle_command(&self, message: IncomingMessage) -> Result<String, RemoteError>;
}

#[async_trait]
pub trait ChannelAdapter: Send + Sync {
    /// Friendly protocol identifier ("telegram", "discord", etc.)
    fn name(&self) -> &str;

    /// Starts long polling or inbound event processing in background
    async fn start_polling(&self, handler: Arc<dyn RemoteCommandHandler>) -> Result<(), RemoteError>;

    /// Sends a text reply to a specific chat (redacting secrets, splitting at 4096 chars)
    async fn send_reply(&self, chat_id: i64, text: &str) -> Result<(), RemoteError>;

    /// Sends a binary file/document attachment (stub for R1/R2 screenshots and logs)
    async fn send_file(
        &self,
        chat_id: i64,
        filename: &str,
        data: &[u8],
        caption: Option<&str>,
    ) -> Result<(), RemoteError>;

    /// Stops polling and cleanly disconnects
    async fn stop(&self) -> Result<(), RemoteError>;

    /// Checks if currently connected and actively polling
    fn is_connected(&self) -> bool;
}
```

---

## 4. Message Formats & Data Models

### Incoming Message
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    pub update_id: i64,
    pub message_id: i64,
    pub sender_id: i64,
    pub chat_id: i64,
    pub timestamp_secs: u64,
    pub text: String,
    pub is_private: bool,
}
```

### Telegram Reply Limits
- Telegram hard limit is **4096 characters** per message.
- The `TelegramAdapter` automatically splits outbound messages on clean paragraph/newline boundaries into sequential chunks $\le 4000$ characters.
- Outbound messages run through `redact_sensitive_content` to scrub API keys, bot tokens, and authorization credentials before transmission.

---

## 5. Task State Machine & Engine Interface

Every plain text request or executable instruction creates a tracked `RemoteTask` governed by a deterministic state machine:

```
          ┌─────────────┐
          │   Queued    │
          └──────┬──────┘
                 │ (engine pickup)
                 ▼
          ┌─────────────┐
          │   Running   │◄──────────────┐
          └──────┬──────┘               │
                 │                      │
       ┌─────────┴─────────┐            │ (/yes <code> confirmed)
       ▼                   ▼            │
┌──────────────┐    ┌─────────────┐     │
│ NeedsInput   ├────┤ Cancelled   │     │
└──────┬───────┘    └─────────────┘     │
       │                   ▲            │
       └───────────────────┴────────────┘
                 │
       ┌─────────┴─────────┐
       ▼                   ▼
┌──────────────┐    ┌─────────────┐
│     Done     │    │   Failed    │
└──────────────┘    └─────────────┘
```

### State Definitions
1. `Queued`: Task record created, placed in FIFO queue.
2. `Running`: Actively executing. *(In R0: Transitions directly to `Done` with response `"received, tool execution isn't enabled yet"`)*.
3. `NeedsInput`: Execution paused pending explicit user confirmation via `/yes <code>` or `/no`.
4. `Done`: Finished successfully with final result.
5. `Failed`: Terminated due to error or tool failure.
6. `Cancelled`: Terminated via `/stop` or user cancellation.

### Task Record
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTask {
    pub id: String,
    pub sender_id: i64,
    pub raw_prompt: String,
    pub state: TaskState,
    pub created_at_secs: u64,
    pub updated_at_secs: u64,
    pub result: Option<String>,
    pub error: Option<String>,
}
```

---

## 6. Remote Command List & Grammar

| Command | Arguments | Behavior in Phase R0 |
|---------|-----------|----------------------|
| *(plain text)* | `<instruction>` | Creates a task record (`TaskState::Done`), logs to audit trail, replies: `"received, tool execution isn't enabled yet"`. |
| `/status` | *(none)* | Queries Windows power APIs (`GetSystemPowerStatus`, `GetTickCount64`) and returns: Laptop awake status, AC vs Battery charge %, uptime, and active task count. |
| `/tasks` | *(none)* | Returns the last 5 tasks with their IDs, timestamps, and terminal states. |
| `/stop` | *(none)* | Immediate emergency abort: transitions all `Queued`, `Running`, and `NeedsInput` tasks to `Cancelled`. Always succeeds. |
| `/pair` | `<code>` | Pairs the bot with the sender's numeric Telegram User ID using the active 5-minute ephemeral code shown in Console. |
| `/yes` | `<code>` | Validates random 4-digit code against active pending confirmation within 120s TTL. In R0: confirms a harmless dummy action. |
| `/no` | *(none)* | Rejects and cancels any active confirmation action. |
| `/help` | *(none)* | Returns command listing and operational instructions. |

---

## 7. Security Model & Defensive Posture

1. **Numeric Telegram User ID Whitelist**:
   - Only messages from the stored `paired_user_id` are processed.
   - All messages from any other user ID are **dropped silently** (zero reply, zero information leakage).
   - Group, channel, and supergroup chats are strictly rejected (`chat.type != "private"`).
2. **Ephemeral Pairing**:
   - Pairing code is 6 characters/digits, randomly generated when unpaired.
   - Displayed **only** in the local Editorial Console UI (never logged, never sent over network).
   - Expires in 5 minutes (300 seconds).
3. **120-Second Message Staleness Drop**:
   - When Ruby reconnects after laptop sleep or offline state, `getUpdates` returns pending backlog.
   - Ruby inspects Telegram message Unix timestamp (`message.date`). If `now - message.date > 120`, the message is **discarded immediately without execution**, and Ruby replies: `"Laptop was offline — resend if still needed"`. Backlog commands are never replayed.
4. **Offset Persistence**:
   - The Telegram update offset is persisted to disk (`~/.ruby/remote_offset.json` on Drive D:). On restart, Ruby resumes from `last_offset + 1`, preventing replay of old commands.
5. **Rate Limiting Unknown Senders**:
   - Senders not matching `paired_user_id` are rate-limited in logs: at most one warning per unknown sender per 60 seconds.
   - Message bodies from unauthorized senders are **never logged**.
6. **Confirmation Plumbing**:
   - Destructive or high-impact actions require random 4-digit confirmation codes (`/yes <code>`), expiring in 120s.
7. **Append-Only Audit Log**:
   - Persisted at `~/.ruby/remote_audit.log`.
   - Records: `timestamp_iso | sender_id | task_id | command_type | outcome`.
   - Never logs tokens or unauthorized message text.

---

## 8. What Phase R1 and Phase R2 Will Need

### Phase R1: Tool Execution via `ruby-policy`
- **Engine Interface**: `TaskManager` will dispatch `Running` tasks to `ruby-policy` instead of returning the R0 placeholder.
- **Interactive Approval**: If a policy rule requires user escalation (`PolicyDecision::NeedsApproval`), the task moves to `NeedsInput`, generates a 4-digit code, and messages the user: `"Tool execution requires confirmation: Run `git push`? Reply /yes 4821 or /no within 2 minutes."`
- **Output Streaming**: Long-running command outputs will send progressive status updates to Telegram before final `Done`.

### Phase R2: Screen Capture & Visual Verification
- **File Attachment Pipeline**: `ChannelAdapter::send_file` will send `.png` desktop snapshots captured by `ruby-platform` via Telegram's `sendDocument` / `sendPhoto`.
- **Command Addition**: `/screen` command to capture and send current multi-monitor screenshot to phone.

---

## 9. Phase R0 Verification Matrix

| Item | Scope | Implementation / Test Coverage | Verification Status |
|:---|:---|:---|:---|
| **1** | **ChannelAdapter Trait & Architecture** | `ChannelAdapter` async trait in `crates/ruby-remote/src/traits.rs`, clean state machine (`Queued`, `Running`, `NeedsInput`, `Done`, `Failed`, `Cancelled`) in `crates/ruby-remote/src/types.rs` and `crates/ruby-remote/src/state_machine.rs`, modular crate layout. | `compiles` |
| **2** | **Long Polling & Offset Persistence** | `getUpdates` client with timeout, atomic offset persistence to `~/.ruby/remote_offset.json`. Tested via mock HTTP server across restarts. | `mock-level` (`test_restart_offset_persistence`) |
| **3** | **Keyring Storage for Bot Token** | `keyring` crate resolution via `ruby_gemini::key_storage` with env fallback. Env fallback tested in unit test; live OS Credential Manager access not asserted in CI. | `UNVERIFIED` (Env fallback tested in `test_bot_token_env_fallback`; OS keyring interaction UNVERIFIED) |
| **4** | **Ephemeral Pairing & 5-Attempt Cap** | `PairingManager` with 6-digit CSPRNG code (`getrandom`), 5-minute TTL, 5-failed-attempt invalidation, and rate-limited sender logging. | `mock-level` (`test_pairing_flow_success_expiry_and_wrong_code`, `test_pairing_failed_attempts_capped_at_five`, `test_code_generators_use_csprng`) |
| **5** | **Numeric User ID Whitelist** | Non-whitelisted senders silently dropped without reply or detail leakage. Rate-limited warning logged without message body. | `mock-level` (`test_unknown_sender_ignored_silently`) |
| **6** | **Task State Machine & Commands** | `/status` queries live Windows power state and uptime; `/tasks` returns last 5 tasks; plain text creates task and replies `"received, tool execution isn't enabled yet"`. | `mock-level` (`test_unknown_sender_ignored_silently`, `test_stop_cancels_all_active_tasks`) |
| **7** | **Emergency Abort `/stop`** | Cancels all active (`Queued`, `Running`, `NeedsInput`) tasks immediately. Never fails or blocks. | `mock-level` (`test_stop_cancels_all_active_tasks`) |
| **8** | **120-Second Message Staleness Drop** | Inbound messages with timestamp older than 120s dropped immediately without execution; reply informs user laptop was offline. | `mock-level` (`test_stale_message_dropped`) |
| **9** | **Destructive Confirmation Plumbing** | `ConfirmationManager` with 4-digit CSPRNG code (`getrandom`), 120s TTL, `/yes <code>` validation, and `/no` cancellation. | `mock-level` (`test_confirmation_plumbing`) |
| **10** | **Token Redaction & Audit Trail** | Bot tokens redacted in logs and error replies via `redact_sensitive_content`. Append-only audit logger records events to `~/.ruby/remote_audit.log`. | `mock-level` (`test_bot_token_redaction`, `test_unknown_sender_ignored_silently`) |
| **11** | **Zero Execution & Clean CI** | Zero child processes or file execution primitives in crate. `cargo fmt --check` (0 diff), `cargo clippy` (0 warnings), workspace tests (65 passed). | `VERIFIED` (`git grep` 0 hits in `src/`, CI `cargo clippy`, `cargo test`) |
| **12** | **Opt-in Live Telegram Roundtrip** | Live Telegram API smoke test under `#[cfg(feature = "live-telegram")]` in `tests/mock_telegram_tests.rs`. Requires `TELEGRAM_BOT_TOKEN` and `TELEGRAM_CHAT_ID`. | `mock-level` (in default CI); `VERIFIED` when run with `--features live-telegram` and live credentials |

