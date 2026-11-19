# Checkpoint 5 - Part 1 Evidence: Lifecycle & Reliability

## L1. Single Instance & Tray Icon
- Implemented `NamedSingleInstance` and `SingleInstanceGuard` trait in `crates/ruby-platform/src/lifecycle.rs`.
- Named mutex `RubyAssistantSingleInstanceMutex` prevents concurrent processes on Windows hosts.
- Tray icon specification includes Open Console, Pause Remote, Stop All, Restart, Quit.
- Tests: `test_single_instance_mock_guard` (VERIFIED).

## L2. Optional Autostart at Login (HKCU Run)
- Implemented `AutostartManager` trait and `MockAutostartManager` in `crates/ruby-platform/src/lifecycle.rs`.
- Target key: `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`, value `RubyAssistant`.
- Real registry operations strictly isolated from unit testing.
- Tests: `test_autostart_mock_enable_and_disable` (VERIFIED).

## L3. Keep-Awake & System Power Status
- Implemented `KeepAwakeManager` trait and `PowerStatusProvider` in `crates/ruby-platform/src/lifecycle.rs`.
- Uses `SetThreadExecutionState` with `ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_AWAYMODE_REQUIRED` when on AC power with remote enabled.
- Querying `/status` on Telegram outputs battery status, uptime, and last sync timestamp.
- Caveats for Modern Standby and lid-close documented in `docs/LIFECYCLE.md`.
- Tests: `test_power_status_query_and_keep_awake` (VERIFIED).

## L4. Telegram Resilience
- Added error classification in `crates/ruby-remote/src/types.rs` and `crates/ruby-remote/src/telegram/client.rs`:
  - `RemoteError::Conflict409`: Pauses polling for 15s to allow duplicate poller resolution.
  - `RemoteError::RateLimited429(secs)`: Respects Telegram `retry_after` parameter.
  - `RemoteError::TokenRevoked`: Halts polling loop on HTTP 401 unauthorized.
- Exponential backoff with CSPRNG jitter on network errors ($500\text{ ms} \dots 60\text{ s}$).
- Persisted offset atomically stored in `%LOCALAPPDATA%\Ruby\telegram_offset.json`.
- Startup announcement (`Ruby online v...`) sent to paired chat.
- Shutdown announcement (`Ruby going offline...`) sent best-effort.
- Telegram `/quit` command with 6-digit confirmation code.
- Tests:
  - `test_telegram_error_classification_types` (VERIFIED)
  - `test_telegram_remote_quit_command_flow` (VERIFIED)

## L5. Crash Handling & Recovery
- Implemented `CrashReporter` in `crates/ruby-platform/src/lifecycle.rs`.
- Writes sanitized crash dumps with location and timestamps to `%LOCALAPPDATA%\Ruby\crashes`.
- Automated recovery in `BackgroundTaskSupervisor::recover_interrupted_jobs()` transitions interrupted jobs from `Running` to `Failed` upon startup.
- Tests: `test_crash_reporter_creates_sanitized_dump` (VERIFIED).

## L6. Session Lock / Unlock Awareness
- Implemented `SessionLockWatcher` trait in `crates/ruby-platform/src/lifecycle.rs`.
- Implemented `TelegramAdapter::on_session_lock_changed` in `crates/ruby-remote/src/telegram/mod.rs`.
- On lock: pauses active Mode A Computer Use tasks and dispatches alert message to paired Telegram chat. Mode B background supervisor tasks continue uninterrupted.
- Tests:
  - `test_session_lock_watcher_mock` (VERIFIED)
  - `test_session_lock_pauses_mode_a_while_mode_b_continues` (VERIFIED)

## L7. Network Watchdog & L8. Graceful Shutdown
- Coordinated shutdown sequence terminates Voice Live, cancels background jobs, stops Playwright sidecars, and invokes `WindowsComputerUseExecutor::release_all_held_inputs()` before exiting.
