# Milestone M5 Acceptance Report: Background Tasks (Mode B Supervisor)

**Milestone**: M5 (Background Tasks / Mode B Supervisor)  
**Author**: Ruby Assistant Team  
**Date**: 2026-11-18  
**Repository**: `Syed-Haseeb97/ruby-agent`  
**Status**: COMPLETE  

---

## 1. Executive Summary & Architecture

Milestone M5 delivers autonomous, durable background task execution (Mode B) in Ruby, completely separated from interactive computer use (Mode A):
- **`DurableJobStore` (`SqliteJobStore`)**: ACID-compliant SQLite queue tracking background jobs (`background_jobs` table) and append-only job execution logs (`job_logs` table). Survived process crashes, supports status tracking: `Queued`, `Running`, `Verifying`, `Completed`, `Failed`, `Cancelled`, `Timeout`.
- **`ExecutionBackend` (`LocalToolBackend`)**: Safe execution backend strictly bound to `workspace_root`.
  - **`GUI_ISOLATION_REQUIRED`**: Immediate failure/refusal if any job attempts interactive GUI or desktop actions (mouse, keyboard, window focus).
  - **Environment Secret Scrubbing**: Strips sensitive environment variables (`KEY`, `TOKEN`, `SECRET`, `PASSWORD`, `CREDENTIAL`) before spawning any child task process.
  - **Advanced Shell Execution**: Strictly disabled by default (`allow_advanced_shell: false`). Rejects pipe chaining (`|`), subshell execution (`$()`, \`\`), and redirection (`>`, `<`) unless explicitly enabled by administrative configuration.
- **`BackgroundTaskSupervisor`**: Asynchronous background worker polling queued jobs, managing execution lifecycle, enforcing per-job execution timeouts, monitoring cancellation tokens, and persisting step logs.
- **User Surfaces & Control**:
  - **Telegram Adapter**: Commands `/jobs` (list recent jobs), `/job <id>` (inspect status), `/cancel <id>` (abort running task), and `/logs <id>` (tail recent logs).
  - **Desktop Tauri API**: Commands `submit_background_job`, `list_background_jobs`, `get_background_job`, `cancel_background_job`, `get_background_job_logs`.
  - **Console UI**: Mode B task submission panel and status cards with static state capsules adhering to Ruby's editorial light mode aesthetics (`#F5F3EE`, `#16181D`, `#C8C2B5`, no blinking animations).

---

## 2. Verification Matrix

| Component | Test Name | Classification | Result |
| :--- | :--- | :--- | :--- |
| **SqliteJobStore** | `test_sqlite_job_store_crud_and_status` | VERIFIED | PASS (ACID SQLite) |
| **SqliteJobStore Logs** | `test_sqlite_job_store_logs` | VERIFIED | PASS (ACID SQLite) |
| **LocalToolBackend** | `test_local_tool_backend_success` | VERIFIED | PASS (Isolated Subprocess) |
| **GUI Isolation** | `test_local_tool_backend_gui_isolation_refusal` | VERIFIED | PASS (`GUI_ISOLATION_REQUIRED` asserted) |
| **Shell Restriction** | `test_local_tool_backend_advanced_shell_disabled_by_default` | VERIFIED | PASS (Refuses `|`, `&&`, `;`) |
| **Supervisor Lifecycle**| `test_background_task_supervisor_lifecycle` | VERIFIED | PASS (Queued -> Running -> Completed) |
| **Supervisor Timeout**  | `test_background_task_supervisor_timeout` | VERIFIED | PASS (Enforces timeout cap) |
| **Supervisor Cancel**   | `test_background_task_supervisor_cancellation` | VERIFIED | PASS (Atomic cancellation token) |
| **Command Parser**      | `test_handle_background_command` | VERIFIED | PASS (`/jobs`, `/job`, `/cancel`, `/logs`) |
| **Telegram Adapter**    | `test_telegram_background_job_commands_lifecycle` | mock-level | PASS (Mock Telegram channel) |
| **Frontend UI Build**   | `npm run build` | VERIFIED | PASS (Vite v6.4.6, 0 errors) |

---

## 3. Negative Invariants & Security Affirmations

1. **GUI Isolation**: Mode B background tasks CANNOT invoke Mode A computer use. Any attempt to access GUI, SendInput, mouse or screen capture triggers `GUI_ISOLATION_REQUIRED` error and immediate job termination.
2. **Environment Scrubbing**: Sensitive environment variables containing credentials or keys are never leaked to task child processes.
3. **No Blinking Indicators**: The frontend UI strictly uses static pill indicators without `animate-pulse` or `animate-ping`.
4. **Dictation Code Untouched**: Zero changes were made to `ruby-dictation` (`keyboard_hook.rs`, `text_injection.rs`, `elevation_check.rs`, `dictation_engine.rs`), strictly preserving Rule G4.
