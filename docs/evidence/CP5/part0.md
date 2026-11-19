# Checkpoint 5 - Part 0 Evidence: Fix Checkpoint 4 Findings

## Z1. Kill Switch Semantics & Hook Code Verification

### 1. Verification of ruby-platform Hook Code
```bash
git diff 456b3a4..HEAD --stat -- crates/ruby-platform/src/keyboard_hook.rs crates/ruby-platform/src/hotkey_machine.rs crates/ruby-platform/src/hook_types.rs crates/ruby-platform/src/text_injection.rs crates/ruby-platform/src/elevation_check.rs
```
Output:
```
 crates/ruby-platform/src/text_injection.rs | 70 +++++++++++++++++++++++++++++-
 1 file changed, 68 insertions(+), 2 deletions(-)
```
- `keyboard_hook.rs`: 0 diffs.
- `hotkey_machine.rs`: 0 diffs.
- `hook_types.rs`: 0 diffs.
- `elevation_check.rs`: 0 diffs.
- `text_injection.rs`: Only diff is the test fixture clipboard backup/restore guard preventing unit tests from touching the user's real OS clipboard. Zero changes were made to hook logic or execution flow.

### 2. Kill Switch Behavior (`crates/ruby-desktop/src/lib.rs`)
- `std::process::exit(0)` removed from `HotKeyAction::KillSwitchTriggered`.
- When Esc is held $\ge 1200\text{ ms}$:
  1. Cancels Computer Use token (`cu_kill_switch.cancel()`).
  2. Stops Voice Live session (`vl_manager.stop().await`).
  3. Cancels active dictation (`dictation_engine.cancel().await`).
  4. Cancels background jobs in supervisor (`bg_supervisor_thread.cancel_all_active()`).
  5. Releases all synthetic mouse buttons and modifier keys via `WindowsComputerUseExecutor::release_all_held_inputs()`.
  6. Emits static event `"kill_switch_triggered"` to Console webview.
  7. Updates Orb operational state to `Idle`.
  8. **Stays RUNNING** - process remains fully active for user commands.
- Quitting is separate:
  - System Tray menu: "Quit"
  - Telegram `/quit`: Requires explicit 6-digit confirmation code (`/yes <code>`).

### 3. Esc Table Specification & Unit Tests
- Documented in `docs/ESC_HIERARCHY.md`.
- Unit tests added to `crates/ruby-desktop/src/commands.rs`:
  - `test_esc_hierarchy_dispatch_mode_a_then_voice_then_dictation` (VERIFIED)
  - `test_esc_hierarchy_all_matrix_rows_tap_and_hold` (VERIFIED)

---

## Z2. Unified 6-Digit Confirmation & Model Leak Prevention

### 1. Code Generation
- `ruby-policy::UnifiedConfirmationManager::generate_secure_code_6_digits()` using CSPRNG `(num % 900_000) + 100_000` $\implies$ strictly 6-digit numeric strings (`100000..=999999`).
- `generate_secure_code_4_digits()` aliased to return 6 digits for backward compatibility.

### 2. Model-Visible Output Sanitization
- `ruby-mcp::McpError::RequiresConfirmation` display message:
  `Tool '{tool_name}' requires user confirmation: {description}. Awaiting user authorization via Console or Telegram.`
  The active confirmation code is omitted from tool output returned to LLM.
- `ruby-core::voice_live`: `[DATA_OUTPUT]` tool response returns confirmation requirement notice with code omitted.
- `ruby-core::tasks`: `execute_command` advanced shell requires confirmation message strictly redacts confirmation code.

### 3. Test Verification
- `test_no_live_pending_confirmation_code_ever_leaks_to_model_visible_outputs` in `crates/ruby-mcp/src/lib.rs` (VERIFIED):
  Generates live pending confirmation for destructive memory deletion, extracts model-visible error message, asserts code is absent and no 6-digit sequence appears in output.

---

## Z3. Screen Recorder Segmentation & Denylist Purge

### 1. Segmented Recording
- Added `segment_seconds: u32` (default: 10s) to `ScreenRecordingConfig`.
- FFmpeg invocation configured with `-f segment -segment_time {s} -reset_timestamps 1`.

### 2. Immediate Segment Purge on Denylist Detection
- In `ScreenRecorder::check_security_denylist_tick`:
  When a denylisted window title/process is detected:
  1. Terminates active child ffmpeg process.
  2. Deletes active output segment immediately from disk (`std::fs::remove_file`).
  3. Transitions state to `PausedByDenylist`.

### 3. Retention & Gated Deletion
- `cleanup_expired_recordings(&self, max_age: Duration)`: purges video files older than retention policy (default: 7 days).
- `delete_all_recordings_with_confirmation(&self, code: &str)`: requires 6-digit confirmation to wipe recording directory.

### 4. Tests
- `test_denylist_tick_sub_500ms_clock_deletes_current_segment_immediately` (VERIFIED)
- `test_screen_recorder_retention_and_delete_with_confirmation` (VERIFIED)
- `test_screen_recording_confirmation_flow_and_denylist_guard` (VERIFIED)

---

## Z4. Test Summary Generator & CI

### 1. Script
- `scripts/summarize_tests.py` handles UTF-8 / UTF-16 byte order marks, extracts per-crate test outcomes, and generates `docs/TEST_SUMMARY.md`.
- Verified execution: `python scripts/summarize_tests.py docs/TEST_RUN_RAW.txt` $\implies$ 258/258 tests passed.

### 2. CI Workflow
- Added `python acceptance/test_secret_scanner_negative.py` to `.github/workflows/ci.yml`.
