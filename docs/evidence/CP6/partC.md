# Checkpoint 6 — Part C: Security Completeness, Fuzzing Matrix & Unsafe Inventory

**Date**: 2026-11-19  
**Status**: COMPLETE (local-only, CI not run)

---

## 1. Security Tools Audit: `cargo audit` & `cargo deny`

### 1.1 Command Outputs
```bash
cargo audit --version
error: no such command: `audit`

cargo deny --version
error: no such command: `deny`
```

### 1.2 Offline Configuration & CI Integration
Neither tool is installed in the local Windows offline development environment.
Per instructions:
1. Created offline configuration `deny.toml` (vulnerability: deny, unmaintained: warn, wildcards: deny, approved licenses list).
2. Created offline configuration `.cargo/audit.toml` (advisories severity threshold: low, unmaintained/unsound: deny).
3. Added both security audit steps to `.github/workflows/ci.yml` so they execute automatically in CI when remote pushes resume.

---

## 2. Fuzzing & Property Testing Matrix for All Parsers

All parsers are fuzzed against pathological handcrafted inputs and pseudo-random corrupted strings (5,000 iterations each):

| Target Parser | Implementation / Scope | Test Suite Location | Iterations & Vectors | Status |
|---|---|---|---|---|
| **Gemini Response Parser** | `ruby_gemini::chat_client::parse_json_emotion` | `crates/ruby-core/tests/fuzz_parsers_matrix.rs:43` | 14 pathological vectors + 5,000 random strings | VERIFIED |
| **Emotion Parser** | `ruby_core::chat::parse_emotion_response` | `crates/ruby-core/tests/fuzz_parsers_matrix.rs:43` | XML emotion tags, markdown fences, empty tags + 5,000 random strings | VERIFIED |
| **Vision Action Parser** | `ruby_actions::computer_use::gemini_vision::GeminiVisionComputerUseModelClient::parse_action_response` | `crates/ruby-core/tests/fuzz_parsers_matrix.rs:72` | Malformed coords, invalid keys, non-JSON markdown + 5,000 random strings | VERIFIED |
| **MCP JSON-RPC Parser** | `ruby_mcp::JsonRpcResponse` line deserialization | `crates/ruby-core/tests/fuzz_parsers_matrix.rs:107` | Partial JSON-RPC, invalid types, oversized buffers + 5,000 random strings | VERIFIED |
| **Telegram Update Parser** | `ruby_remote::telegram::TelegramAdapter::process_update` | `crates/ruby-remote/tests/fuzz_telegram_grammar.rs:130` | Malformed update IDs, group chat bypass, missing message fields + 5,000 random strings | VERIFIED |

---

## 3. Proptest: Secret Redaction (1,000 Cases with Zero-Width Spaces & Unicode Whitespace)

Implementation: `crates/ruby-core/src/logging.rs:298` (`test_proptest_secret_redaction_1000_cases_with_zero_width_spaces_and_unicode_whitespace`).

- **Test Parameters**: 1,000 randomized cases using deterministic LCG pseudo-random generator.
- **Evasion Attacks Tested**:
  - Secrets split by zero-width spaces (`\u{200B}`, `\u{200C}`, `\u{FEFF}`, `\u{200D}`).
  - Unicode whitespace injections (`\u{00A0}`, `\u{2000}`, `\u{2003}`, `\u{2009}`, `\u{3000}`).
  - Target classes: Google Gemini API keys (`AIza...`), GitHub PATs (`ghp_...`), OpenAI/Anthropic keys (`sk-...`), FreeLLMAPI keys (`freellmapi-...`).
- **Assertion**:
  - Zero raw tokens leak into sanitized output (`assert!(!clean.contains(...))`).
  - Redaction marker `[REDACTED]` is strictly present in every output.
- **Result**: `1 passed; 0 failed; finished in 0.04s`.

---

## 4. Unsafe Code Inventory

Complete inventory written to `docs/UNSAFE_INVENTORY.md`.
- **Total Points**: 39 points across `ruby-platform`, `ruby-actions`, `ruby-desktop`, `ruby-core`, and `ruby-remote`.
- **Zero Raw Pointers Exposed**: All raw Win32 calls are encapsulated behind safe RAII types (`JobObjectProcessTree`, `UserClipboardGuard`, `NamedSingleInstance`).
- **Zero Unsound Memory Leaks**: Handles are strictly closed via `Drop`.

---

## 5. Comprehensive IPC Command Audit Table (35 Commands)

Every `#[tauri::command]` in `crates/ruby-desktop/src/commands.rs`:

| Command Name | Arguments | Caller | State Mutated | Policy Check | Confirmation Required? | Test Verification |
|---|---|---|---|---|---|---|
| `get_orb_state` | `State<AppState>` | Frontend Orb | None (Read) | None (Local query) | No | `commands::tests::test_orb_state` |
| `set_orb_state` | `AppHandle, State<AppState>, OrbVisualState` | Frontend Orb | `AppState.orb_state` | None (UI display) | No | `commands::tests::test_orb_state` |
| `toggle_orb_click_through` | `Window, State<AppState>` | Frontend Orb | `AppState.click_through`, Win32 style | None (UI toggle) | No | `commands::tests::test_toggle_click_through` |
| `get_remote_status` | `State<AppState>` | Settings UI | None (Read) | None (Local query) | No | `commands::tests::test_get_remote_status` |
| `disconnect_remote` | `State<AppState>` | Settings UI | `AppState.remote_adapter` pairing | None (Local owner) | No | `commands::tests::test_disconnect_remote` |
| `send_chat_message` | `AppHandle, State<AppState>, String` | Chat UI | `AppState.chat_session` | Tool policy on model tool calls | Destructive model actions require confirmation | `commands::tests::test_model_tool_call_policy_gating_and_data_return` |
| `cancel_speech` | `AppHandle, State<AppState>` | Chat UI / Hotkey | `AppState.tts_playback` | None | No | `commands::tests::test_cancel_speech` |
| `execute_tool` | `AppHandle, State<AppState>, String, Value` | Model / UI | MCP tool execution | `ToolSecurityPolicy` (allowlist + bounds) | Write/destructive tools require confirmation | `ruby-mcp::tests::test_write_and_destructive_tools_require_confirmation` |
| `confirm_tool_execution` | `State<AppState>, String` | Console UI | Consumes pending confirmation | `UnifiedConfirmationManager` (code verification) | N/A (Executes confirmed action) | `ruby-mcp::tests::test_write_and_destructive_tools_require_confirmation` |
| `add_memory` | `State<AppState>, String, String` | Chat UI | `AppState.memory_store` | Input length & secret sanitization | No | `ruby-memory::tests::test_memory_crud` |
| `search_memories` | `State<AppState>, String, usize` | Chat UI | None (Read) | None (Local query) | No | `ruby-memory::tests::test_memory_crud` |
| `delete_memory` | `State<AppState>, i64, Option<String>` | Memory UI | `AppState.memory_store` record | Confirmation required | Yes (6-digit CSPRNG code) | `commands::voice_live_tests::test_console_ipc_delete_memory_requires_confirmation_then_deletes` |
| `wipe_all_memories` | `State<AppState>, Option<String>` | Memory UI | `AppState.memory_store` (all records) | Confirmation required (shows count) | Yes (6-digit CSPRNG code) | `commands::voice_live_tests::test_console_ipc_wipe_all_memories_shows_count_requires_confirmation_then_wipes` |
| `start_computer_use_task` | `AppHandle, State<AppState>, String, bool` | Console UI | `AppState.computer_use_active_goal` | Dry-run default; elevation / denylist check | User initiation | `ruby-actions::computer_use::agent_loop::tests::test_agent_loop_success_path` |
| `stop_computer_use_task` | `AppHandle, State<AppState>` | Console UI / Esc | `AppState.computer_use_kill_switch` | Emergency kill switch | No | `ruby-actions::computer_use::agent_loop::tests::test_kill_switch_during_drag_releases_mouse` |
| `get_computer_use_status` | `State<AppState>` | Console UI | None (Read) | None (Audit record query) | No | `ruby-actions::computer_use::agent_loop::tests::test_audit_log_hygiene_never_records_typed_text_or_screenshots` |
| `save_orb_snapshot` | `String, String` | Test / Dev | Disk artifact in `%LOCALAPPDATA%\Ruby` | Canonical path validation (no traversal) | No | `commands::tests::test_save_orb_snapshot` |
| `start_voice_live` | `State<AppState>, AppHandle` | Console Button / Hotkey | `AppState.voice_live_manager` | Live audio session initiation | Explicit user start required | `commands::voice_live_tests::test_voice_live_command_start_and_stop` |
| `stop_voice_live` | `State<AppState>` | Console Button / Esc | `AppState.voice_live_manager` | Session termination | No | `commands::voice_live_tests::test_voice_live_command_start_and_stop` |
| `get_voice_live_status` | `State<AppState>` | Console UI | None (Read) | None | No | `commands::voice_live_tests::test_get_voice_live_status_idle_string` |
| `get_voice_live_transcript` | `State<AppState>` | Console UI | None (Read) | Redacted transcript | No | `ruby-core::voice_live::tests::test_voice_live_audio_and_transcript_never_logged_to_disk` |
| `push_voice_live_audio` | `State<AppState>, Vec<u8>` | Mic capture | Audio stream buffer | Refused unless Voice Live active | No | `commands::voice_live_tests::test_push_mic_audio_before_start_errors` |
| `submit_background_job` | `State<AppState>, String` | Console UI | `AppState.bg_supervisor` | Command security policy | Advanced shell OFF by default | `ruby-core::tasks::tests::test_background_supervisor_lifecycle` |
| `list_background_jobs` | `State<AppState>` | Console UI | None (Read) | None | No | `ruby-core::tasks::tests::test_background_supervisor_lifecycle` |
| `get_background_job` | `State<AppState>, String` | Console UI | None (Read) | None | No | `ruby-core::tasks::tests::test_background_supervisor_lifecycle` |
| `cancel_background_job` | `State<AppState>, String` | Console UI | Job status -> Cancelled | Job ownership validation | No | `ruby-core::tasks::tests::test_background_supervisor_lifecycle` |
| `get_background_job_logs` | `State<AppState>, String, usize` | Console UI | None (Read) | Output redaction | No | `ruby-core::tasks::tests::test_background_supervisor_lifecycle` |
| `run_doctor` | `Option<bool>` | Settings UI | None (Diagnostic report) | Read-only; opt-in network | No | `ruby-core::doctor::tests::test_doctor_output_is_sanitized` |
| `get_app_config` | None | Settings UI | None (Read) | Resolves `%LOCALAPPDATA%\Ruby\config.json` | No | `commands::voice_live_tests::test_save_app_config_succeeds_with_safe_defaults` |
| `save_app_config` | `AppConfigPayload, Option<String>` | Settings UI | Overwrites config file | Dangerous flags require confirmation; whole drive root warning | Yes (if dangerous flags toggled) | `commands::voice_live_tests::test_save_app_config_requires_confirmation_for_dangerous_flags` |
| `get_secrets_status` | None | Settings UI | None (Read) | Returns boolean flags only (`*_set`), never plaintext | No | `commands::voice_live_tests::test_get_secrets_status_never_contains_values` |
| `is_first_run` | None | Console UI | None (Read) | Checks config file existence | No | `commands::tests::test_first_run` |
| `get_recent_logs` | `State<AppState>, Option<String>, usize` | Console UI | None (Read) | Global secret redaction | No | `commands::voice_live_tests::test_observability_redacted_logger_and_metrics_ipc` |
| `get_system_metrics` | `State<AppState>` | Console UI | None (Read) | Telemetry snapshot | No | `commands::voice_live_tests::test_observability_redacted_logger_and_metrics_ipc` |
| `delete_recordings` | `State<AppState>, Option<String>` | Console UI | Disk recordings purged | Confirmation required | Yes (6-digit CSPRNG code) | `commands::voice_live_tests::test_console_ipc_delete_recordings_requires_confirmation_then_deletes` |
