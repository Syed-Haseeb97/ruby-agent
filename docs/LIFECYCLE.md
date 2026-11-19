# Lifecycle & Reliability Specification

This document details the lifecycle management, single-instance enforcement, autostart, power state handling, crash recovery, and Telegram resilience mechanisms implemented for Ruby Assistant (Checkpoint 5, Part 1).

---

## L1. Single-Instance & System Tray

### 1. Named Mutex Single-Instance Guard
- Enforced on Windows via `CreateMutexW(None, true, "RubyAssistantSingleInstanceMutex")`.
- If `GetLastError() == ERROR_ALREADY_EXISTS`, the secondary instance immediately aborts with an informational log and exit code 0.
- Safe tests use `MockSingleInstanceGuard` to avoid interfering with any running instance.

### 2. System Tray & Minimized Startup
- The application creates a system tray icon with the following standard menu items:
  1. **Open Console**: Unhides and focuses the primary Console window.
  2. **Pause Remote**: Temporarily suspends inbound Telegram long polling.
  3. **Stop All**: Global emergency stop aborting Computer Use, Voice Live, and background jobs.
  4. **Restart**: Gracefully shuts down subsystems and restarts the process.
  5. **Quit**: Initiates coordinated graceful shutdown.
- Configurable `start_minimized: bool` (default `false`) starts the application with the Console window minimized/hidden to the system tray.

---

## L2. Optional Autostart at Login (HKCU Run)

- **Registry Key**: `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`.
- **Value Name**: `RubyAssistant`.
- **Value Data**: `"C:\Path\To\ruby.exe" --minimized`.
- **Safety Rule**: The real registry is NEVER modified during tests or standard build runs. All autostart operations are mediated by the `AutostartManager` trait. Unit tests exclusively use `MockAutostartManager`. Real registry writes only occur when explicitly toggled by the user via Console Settings or CLI `--autostart`.

---

## L3. Keep-Awake & System Power Status

### 1. Keep-Awake Implementation
- When remote access is enabled and the laptop is plugged into AC power, Ruby maintains system execution state via `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_AWAYMODE_REQUIRED)`.
- Prevents the operating system from entering deep sleep while remote commands are awaiting execution.
- Gated behind `keep_awake: bool` in `remote_config.json` and a toggle in Console Settings.

### 2. Laptop Lid-Close & Modern Standby Caveats
> [!IMPORTANT]
> **Modern Standby (S0 Low Power Idle) & Lid-Close Behavior**:
> 1. **Lid Close**: Windows default behavior upon closing a laptop lid is to enter Sleep or Hibernate, which powers down the CPU regardless of `SetThreadExecutionState`. To operate Ruby with the laptop lid closed while in your room, you must configure Windows Control Panel:
>    `Control Panel -> Power Options -> Choose what closing the lid does -> When I close the lid (Plugged in): Do nothing`.
> 2. **Modern Standby (S0)**: Modern laptops support S0ix Connected Standby. Under Modern Standby, Windows may periodically throttle background threads or disconnect Wi-Fi if running on battery. Ruby's Keep-Awake requests `ES_AWAYMODE_REQUIRED` on AC power to keep network interfaces and background workers active.

### 3. Remote `/status` Telemetry
Querying `/status` on Telegram returns real-time diagnostics:
- **Power**: AC plugged / Charging / Battery percentage.
- **Uptime**: System uptime formatted in hours and minutes.
- **Last Sync**: Seconds elapsed since the last successful Telegram poll.
- **Active Tasks**: Count of running Mode A and Mode B tasks.

---

## L4. Telegram Polling Resilience

### 1. Exponential Backoff with Jitter
- Initial retry backoff: 500 ms.
- Backoff multiplier: $2\times$ per failure up to 60 seconds.
- Cryptographic jitter: $\pm 0\dots 500\text{ ms}$ calculated via CSPRNG to prevent thundering herd.

### 2. HTTP Error Handling
- **HTTP 409 Conflict**: Occurs if a duplicate bot instance connects. Long polling pauses for 15 seconds to allow collision resolution without hammering Telegram.
- **HTTP 429 Too Many Requests**: Parses `parameters.retry_after` from response body and suspends polling for the exact duration demanded by Telegram.
- **HTTP 401 Unauthorized**: Bot token was revoked or invalid. Permanently stops polling and flags `TokenRevoked` to the host console.

### 3. State & Announcements
- **Persisted Offset**: The latest `update_id` is atomically flushed to `%LOCALAPPDATA%\Ruby\telegram_offset.json` after every processed batch to prevent replay attacks on restart.
- **Startup Announcement**: Sends `"Ruby online (v{version}, mode_a: {on/off})"` to the paired Telegram chat upon startup.
- **Shutdown Announcement**: Sends `"Ruby going offline (shutting down)."` before releasing resources.

---

## L5. Crash Handling & Recovery

### 1. Sanitized Crash Dumps
- Panics trigger a custom panic hook that serializes stack traces, location, and metadata to:
  `%LOCALAPPDATA%\Ruby\crashes\crash_{timestamp}_{uuid}.json`.
- All credentials, bot tokens, API keys, and sensitive paths are scrubbed through a redaction pipeline prior to serialization.

### 2. Startup Task Recovery
- On initialization, `BackgroundTaskSupervisor::recover_interrupted_jobs()` inspects the local task store.
- Any jobs remaining in `Running` state from an unexpected process termination are transitioned to:
  `TaskStatus::Failed("Task was interrupted by system shutdown/restart (recovered on startup)")`.

---

## L6. Session Lock / Unlock Awareness

- Watches Windows Terminal Services notifications (`WTSRegisterSessionNotification`) for `WTS_SESSION_LOCK` and `WTS_SESSION_UNLOCK`.
- **On Lock**:
  - Automatically pauses any active Mode A (Computer Use) task.
  - Sends immediate Telegram alert to paired user: `"⚠️ Desktop session locked: Mode A autonomous task paused for security. Mode B background tasks continue running."`
  - **Mode B background jobs continue running uninterrupted.**
- **On Unlock**: Mode A tasks remain safely paused until explicitly resumed by user.

---

## L7. Network Watchdog

- A background heartbeat verifies connectivity to network gateway.
- If connectivity drops, emits `"network_status_changed"` with `{"online": false}` to display an offline banner in the Console UI.
- On reconnect, clears the banner and syncs status.

---

## L8. Coordinated Graceful Shutdown

Graceful shutdown (`graceful_shutdown()`) guarantees that:
1. Mode A Computer Use loop aborts and drops active goals.
2. Voice Live sessions close audio output and WebSocket handles.
3. Background supervisor cancels running child tasks.
4. Tool manager terminates Playwright sidecar processes.
5. `WindowsComputerUseExecutor` releases all held synthetic mouse and keyboard inputs.
6. Tracing logs are flushed to disk.
7. Named mutex handle is closed cleanly.
