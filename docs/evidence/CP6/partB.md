# Checkpoint 6 — Part B: Part 0 Proofs & Recorder Hardening

**Date**: 2026-11-19
**Status**: COMPLETE (local-only, CI not run)

---

## 1. Platform Hook Integrity and Zero `process::exit` Proof

### 1.1 Diff Stat on the 5 Protected Hook Files
Command executed:
```bash
git diff 456b3a4..HEAD --stat -- crates/ruby-platform/src/keyboard_hook.rs crates/ruby-platform/src/hotkey_machine.rs crates/ruby-platform/src/hook_types.rs crates/ruby-platform/src/text_injection.rs crates/ruby-platform/src/elevation_check.rs
```
Output:
```text
 crates/ruby-platform/src/text_injection.rs | 70 +++++++++++++++++++++++++++++-
 1 file changed, 68 insertions(+), 2 deletions(-)
```

### 1.2 Minimal Diff and Justification
The 4 core hook files (`keyboard_hook.rs`, `hotkey_machine.rs`, `hook_types.rs`, `elevation_check.rs`) have **0 modified lines**.
In `crates/ruby-platform/src/text_injection.rs`, production code is completely untouched. The diff is strictly inside `mod tests`, introducing `UserClipboardGuard` and `CLIPBOARD_TEST_MUTEX` to snapshot and restore the user's actual clipboard content upon test completion, preventing test corruption of real clipboard state:
```rust
diff --git a/crates/ruby-platform/src/text_injection.rs b/crates/ruby-platform/src/text_injection.rs
--- a/crates/ruby-platform/src/text_injection.rs
+++ b/crates/ruby-platform/src/text_injection.rs
@@ -351,22 +351,88 @@ mod tests {
+    static CLIPBOARD_TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
+
     #[test]
     fn test_clipboard_multiformat_backup_and_restore() {
+        let _test_lock = CLIPBOARD_TEST_MUTEX.lock().unwrap();
...
+        // 0. Backup user's actual clipboard content so tests NEVER overwrite user's clipboard!
+        struct UserClipboardGuard(Vec<ClipboardSnapshotEntry>);
+        impl Drop for UserClipboardGuard { ... }
+        let _user_guard = UserClipboardGuard(user_original_snapshot);
```

### 1.3 `process::exit` Grep Proof
Command executed:
```bash
git grep -n "process::exit"
```
Output:
```text
crates/ruby-platform/examples/hook_spike.rs:233:            std::process::exit(1);
docs/ESC_HIERARCHY.md:33:  - **Does NOT exit the process (`process::exit` is strictly prohibited on this path).**
docs/evidence/CP5/part0.md:21:- `std::process::exit(0)` removed from `HotKeyAction::KillSwitchTriggered`.
```
**Proof**: Zero occurrences in production code. The single occurrence is in an example spike harness (`hook_spike.rs`).

---

## 2. Esc Hierarchy Table with Test Verification per Row

Implementation: `crates/ruby-desktop/src/commands.rs:1360` (`dispatch_esc_hierarchy`) and tests `test_esc_hierarchy_dispatch_mode_a_then_voice_then_dictation` + `test_esc_hierarchy_all_matrix_rows_tap_and_hold`.

| Row | Current Active State | Esc Action | Expected Outcome | Test Function & Location | Status |
|---|---|---|---|---|---|
| 1 | Mode A (Computer Use active) | Tap | Cancels Mode A immediately via KillSwitchToken; releases held mouse/keys; does NOT cancel dictation | `crates/ruby-desktop/src/commands.rs:1403` (`test_esc_hierarchy_all_matrix_rows_tap_and_hold`) | VERIFIED |
| 2 | Mode V (Voice Live active, Mode A idle) | Tap | Gracefully stops Voice Live session (`VoiceLiveManager::stop`); does NOT cancel dictation | `crates/ruby-desktop/src/commands.rs:1412` (`test_esc_hierarchy_all_matrix_rows_tap_and_hold`) | VERIFIED |
| 3 | Mode D (Dictation active, Mode A/V idle) | Tap | Cancels current dictation audio capture/streaming without pasting | `crates/ruby-desktop/src/commands.rs:1422` (`test_esc_hierarchy_all_matrix_rows_tap_and_hold`) | VERIFIED |
| 4 | Mode B (Background Task active) | Tap | Background jobs are NOT cancelled by Esc tap; preserves long-running tasks | `crates/ruby-desktop/src/commands.rs:1432` (`test_esc_hierarchy_all_matrix_rows_tap_and_hold`) | VERIFIED |
| 5 | Any / All States | Hold ($\ge$ 1.5s) | Global emergency stop: cancels Mode A, stops Voice Live, cancels Dictation, cancels background supervisor jobs; process stays alive | `crates/ruby-desktop/src/commands.rs:1442` (`test_esc_hierarchy_all_matrix_rows_tap_and_hold`) | VERIFIED |

---

## 3. Z2: Leakage Scan of Model-Visible Outputs for Pending Confirmation Codes

Implementation: `crates/ruby-mcp/src/lib.rs:1222` (`test_no_live_pending_confirmation_code_ever_leaks_to_model_visible_outputs`).

The test registers an action requiring confirmation in `UnifiedConfirmationManager` (generating a live 6-digit CSPRNG code), then executes:
1. Model-driven tool execution response.
2. Error response emitted when confirmation is required.
3. Memory search output (`search_memories`).
4. Redacted log output (`RedactedJsonLogger`).

**Assertion**:
Every model-visible string output is checked:
`assert!(!model_visible_text.contains(&code))`
Result:
`test tests::test_no_live_pending_confirmation_code_ever_leaks_to_model_visible_outputs ... ok`

---

## 4. Z3: Screen Recorder Hardening & Console IPC Deletion Confirmation

Implementation: `crates/ruby-actions/src/screen_recorder.rs` and `crates/ruby-desktop/src/commands.rs`.

### 4.1 Segment Length Configuration
Default segment length is configurable, defaulting to 10 seconds for test isolation and 300 seconds for production rolling buffers:
`ScreenRecordingConfig::segment_seconds` (tested in `test_denylist_tick_sub_500ms_clock_deletes_current_segment_immediately`).

### 4.2 Sub-500ms Denylist Tick Heartbeat & Segment Deletion on Detection
- Verified in `crates/ruby-actions/src/screen_recorder.rs:401`: `test_denylist_tick_sub_500ms_clock_deletes_current_segment_immediately`.
- Simulated clock tick occurs at 250ms ($\le 500$ms).
- When a denylisted window (`SensitiveBank`) appears on the visible window list, the recorder immediately stops the ffmpeg recording process and purges the active `.mp4` segment from disk (`assert!(!out_file.exists())`).

### 4.3 Retention Auto-Deletion
- Implementation: `ScreenRecorder::cleanup_expired_recordings(max_age: Duration)`.
- Verified in `crates/ruby-actions/src/screen_recorder.rs:437`: `test_screen_recorder_retention_and_delete_with_confirmation`.
- Files older than the retention threshold are automatically pruned during periodic maintenance.

### 4.4 Console IPC `delete_recordings` Confirmation
- Implementation: `crates/ruby-desktop/src/commands.rs:1165` (`delete_recordings` and `delete_recordings_core`).
- Verified in `crates/ruby-desktop/src/commands.rs:1545`: `test_console_ipc_delete_recordings_requires_confirmation_then_deletes`.
  1. `delete_recordings_core(..., None)` returns `Err("Delete all recordings requires confirmation. Code: <6-digit-code>")` and leaves recordings intact.
  2. `delete_recordings_core(..., Some("wrong"))` fails confirmation and leaves recordings intact.
  3. `delete_recordings_core(..., Some(code))` consumes confirmation and deletes all recordings on disk.
