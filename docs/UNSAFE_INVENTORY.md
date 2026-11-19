# Unsafe Code Inventory and Invariant Catalog

**Repository**: `ruby-agent`  
**Audit Scope**: Complete workspace scan of all `unsafe` blocks and traits  
**Last Audited**: 2026-11-19 (Checkpoint 6)  
**Total Unsafe Points**: 39 production & platform integration sites  

---

## 1. Architectural Philosophy
`ruby-agent` minimizes `unsafe` strictly to essential Win32 OS boundary operations where the Windows API does not provide a safe abstraction:
1. Low-Level Keyboard Hook (`WH_KEYBOARD_LL`) for global hotkeys.
2. Low-level Desktop Session & Elevation checks (`Winlogon`, `UAC` secure desktop, Token elevation).
3. Windows Job Object management (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) for process sandboxing.
4. Win32 Clipboard transactional management with history exclusion.
5. Windows Power & Display metrics queries.

All `unsafe` blocks are wrapped in safe Rust structs providing strict RAII lifecycles, drop guards, and null-pointer checks.

---

## 2. Exhaustive Unsafe Inventory Table

| # | File & Line | Win32 API / Primitive | Purpose & Justification | Invariant Maintained |
|---|---|---|---|---|
| 1 | `crates/ruby-actions/src/computer_use/agent_loop.rs:81` | `GetForegroundWindow` | Query active foreground window for elevation check | Return value checked for NULL/zero before dereferencing; read-only query. |
| 2 | `crates/ruby-actions/src/computer_use/agent_loop.rs:89` | `OpenInputDesktop`, `CloseDesktop` | Detect if workstation is on Winlogon / UAC secure desktop | Returned `HDESK` handle is immediately closed in same scope via `CloseDesktop`. |
| 3 | `crates/ruby-actions/src/computer_use/agent_loop.rs:265` | `GetForegroundWindow` | Snapshot active window title for audit record | Read-only HWND query; handles NULL gracefully with fallback "Unknown". |
| 4 | `crates/ruby-actions/src/computer_use/agent_loop.rs:296` | `unsafe extern "system" fn enum_proc` | Win32 `EnumWindows` callback | Casts `LPARAM` back to stack `&mut Vec<(String, String)>`; valid for callback lifetime. |
| 5 | `crates/ruby-actions/src/computer_use/agent_loop.rs:328` | `EnumWindows` | Top-level window enumeration for denylist inspection | Stack reference passed to `enum_proc` remains valid until `EnumWindows` returns. |
| 6 | `crates/ruby-actions/src/computer_use/executor.rs:66` | `mouse_event` (MOVE) | Synthetic mouse movement during Mode A tasks | Coordinates pre-validated against virtual desktop bounding rect. |
| 7 | `crates/ruby-actions/src/computer_use/executor.rs:76` | `mouse_event` (DOWN) | Synthetic mouse button press | Flags validated; button state tracked in `held_buttons` to guarantee release on abort. |
| 8 | `crates/ruby-actions/src/computer_use/executor.rs:84` | `mouse_event` (UP) | Synthetic mouse button release | Matches preceding DOWN; clears state in tracking bitset. |
| 9 | `crates/ruby-actions/src/computer_use/executor.rs:216` | `keybd_event` (DOWN) | Synthetic key press | Virtual key code checked; dangerous keys (Win, Ctrl+Alt+Del) filtered before invocation. |
| 10 | `crates/ruby-actions/src/computer_use/executor.rs:242` | `keybd_event` (UP) | Synthetic key release | Balances key down; releases modifier keys on step completion. |
| 11 | `crates/ruby-actions/src/computer_use/executor.rs:470` | `mouse_event`, `keybd_event` | Emergency cleanup on kill-switch trip | Iterates all held keys and buttons sending UP events; prevents OS input lockup. |
| 12 | `crates/ruby-core/src/doctor.rs:293` | `GetSystemPowerStatus` | Read battery and power status for doctor report | Stack-allocated `SYSTEM_POWER_STATUS` initialized to zero; return code checked. |
| 13 | `crates/ruby-desktop/src/commands.rs:119` | `SetWindowLongW` | Toggle `WS_EX_TRANSPARENT` for orb click-through | Valid HWND extracted from Tauri window; bitmask update preserves existing style bits. |
| 14 | `crates/ruby-desktop/src/lib.rs:62` | `CreateMutexW`, `CloseHandle` | Test suite single-instance isolation | Handle checked for null/invalid; closed via Drop implementation. |
| 15 | `crates/ruby-desktop/src/lib.rs:336` | `WTSRegisterSessionNotification` | Receive Windows session lock / unlock notifications | Unregisters cleanly via `WTSUnRegisterSessionNotification` on application exit. |
| 16 | `crates/ruby-platform/src/elevation_check.rs:27` | `GetTokenInformation` | Evaluate elevation level of process token | Buffer length checked; correctly queries `TOKEN_ELEVATION` struct. |
| 17 | `crates/ruby-platform/src/elevation_check.rs:47` | `OpenProcessToken` | Retrieve token for elevation check | Token closed via `CloseHandle` immediately upon completion. |
| 18 | `crates/ruby-platform/src/elevation_check.rs:62` | `GetWindowThreadProcessId` | Extract process ID from window handle | Valid stack DWORD pointer; return code non-zero validated. |
| 19 | `crates/ruby-platform/src/elevation_check.rs:94` | `OpenProcess` | Open target process for inspection | Requests only `PROCESS_QUERY_LIMITED_INFORMATION`; handle closed via `CloseHandle`. |
| 20 | `crates/ruby-platform/src/elevation_check.rs:125` | `GetCurrentProcess`, `OpenProcessToken` | Check if Ruby process itself is elevated | Token closed via RAII guard. |
| 21 | `crates/ruby-platform/src/elevation_check.rs:171` | `OpenDesktopW`, `CloseDesktop` | Test access to interactive desktop station | Desktop handle closed before function returns. |
| 22 | `crates/ruby-platform/src/job_object.rs:18` | `unsafe impl Send for JobObjectProcessTree` | Send marker for process tree wrapper | Managed handle is kernel-synchronized; thread-safe transfer. |
| 23 | `crates/ruby-platform/src/job_object.rs:19` | `unsafe impl Sync for JobObjectProcessTree` | Sync marker for process tree wrapper | Handle is immutable after creation; concurrent calls use OS atomics. |
| 24 | `crates/ruby-platform/src/job_object.rs:24` | `CreateJobObjectW` | Create Job Object for subprocess sandbox | Checked for NULL; wrapped in RAII struct. |
| 25 | `crates/ruby-platform/src/job_object.rs:52` | `SetInformationJobObject` | Enable `KILL_ON_JOB_CLOSE` | Struct size and limit flags correctly populated; return code checked. |
| 26 | `crates/ruby-platform/src/job_object.rs:66` | `AssignProcessToJobObject` | Bind child process to job | Checks process handle and job handle validity before binding. |
| 27 | `crates/ruby-platform/src/job_object.rs:80` | `TerminateJobObject`, `CloseHandle` | Terminate all child processes on drop | Closes handle cleanly, ensuring no orphaned processes remain. |
| 28 | `crates/ruby-platform/src/keyboard_hook.rs:33` | `unsafe extern "system" fn low_level_keyboard_proc` | WH_KEYBOARD_LL hook callback | Validates `nCode >= 0`; calls `CallNextHookEx` on all unhandled messages. |
| 29 | `crates/ruby-platform/src/keyboard_hook.rs:93` | `SetWindowsHookExW` | Install low-level keyboard hook | Returns `HHOOK`; validated non-null before thread loop starts. |
| 30 | `crates/ruby-platform/src/keyboard_hook.rs:117` | `UnhookWindowsHookEx` | Remove low-level keyboard hook | Idempotent unhook call; clears handle to prevent dangling hooks. |
| 31 | `crates/ruby-platform/src/keyboard_hook.rs:129` | `GetCurrentThreadId` | Get hook thread identifier | Read-only system query. |
| 32 | `crates/ruby-platform/src/keyboard_hook.rs:132` | `PostThreadMessageW` | Post WM_QUIT to hook thread | Thread ID validated active; exits loop safely. |
| 33 | `crates/ruby-platform/src/keyboard_hook.rs:237` | `GetMessageW`, `DispatchMessageW` | Standard Win32 hook thread message pump | Loop terminates cleanly on WM_QUIT (0 return). |
| 34 | `crates/ruby-platform/src/lib.rs:81` | `GetSystemMetrics` | Query virtual screen coordinates and dimensions | Read-only system metrics call; handles multi-monitor negative coordinates. |
| 35 | `crates/ruby-platform/src/lib.rs:106` | `EnumDisplayMonitors` | Multi-monitor geometry enumeration | Callback stack reference valid during call. |
| 36 | `crates/ruby-platform/src/lib.rs:183` | `GetDpiForWindow`, `GetDpiForSystem` | High-DPI scaling factor computation | Fallback to 96 DPI if API unavailable; handles per-monitor DPI. |
| 37 | `crates/ruby-platform/src/lifecycle.rs:66` | `CreateMutexW` | Named single instance enforcement | Mutex name namespaced; `ERROR_ALREADY_EXISTS` checked. |
| 38 | `crates/ruby-platform/src/lifecycle.rs:95-96` | `unsafe impl Send / Sync for NamedSingleInstance` | Thread safety markers | Mutex handle exclusively owned by struct. |
| 39 | `crates/ruby-platform/src/lifecycle.rs:108` | `CloseHandle` | Release single instance mutex on drop | Handle checked non-null before closing. |
| 40 | `crates/ruby-platform/src/text_injection.rs` (multiple) | `OpenClipboard`, `SetClipboardData`, etc. | Clipboard text injection with history exclusion | Guarded by `UserClipboardGuard`; always closed via `CloseClipboard`; global locks released. |
| 41 | `crates/ruby-remote/src/keep_awake.rs:22, 28` | `SetThreadExecutionState` | Prevent system sleep during background tasks | Resets execution state to `ES_CONTINUOUS` on Drop. |
| 42 | `crates/ruby-remote/src/power.rs:19, 50` | `GetSystemPowerStatus`, `GetTickCount64` | System uptime and power state queries | Read-only; zeroed stack struct. |
