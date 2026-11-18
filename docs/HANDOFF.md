# Ruby Assistant — Checkpoint & Milestone Handoff

**Author**: Ruby Assistant Team  
**Date**: 2026-11-18  
**Repository**: `Syed-Haseeb97/ruby-agent`  
**Current Milestone**: M5 (Background Tasks / Mode B Supervisor) & V1 Voice Live Completed  
**Next Milestone**: Ready for Production Deployment / User Validation

---

## 1. Status Overview

All Part A hardening items, Milestone M4 gaps, Milestone M5 Background Tasks (Mode B Supervisor), and V1 Voice Live hardening have been addressed, implemented, and verified across all subsystems.

### V1 Voice Live Relabeling & Scope Clarification
- In earlier reports, Voice Live and bidirectional streaming was colloquially referred to alongside M5. Per explicit user direction, the Voice Live work is formally relabeled as **"V1 Voice Live"** and documented in [V1_VOICE_LIVE_ACCEPTANCE.md](file:///D:/projects/ruby-agent/docs/V1_VOICE_LIVE_ACCEPTANCE.md).
- **Milestone M5** strictly designates the **Background Tasks (Mode B Supervisor)** per Part C of the project plan, featuring `DurableJobStore` (SQLite), `LocalToolBackend` (`GUI_ISOLATION_REQUIRED`, advanced shell disabled by default, environment secret scrubbing), `BackgroundTaskSupervisor` async queue worker, Telegram commands (`/jobs`, `/job`, `/cancel`, `/logs`), and desktop Console UI, documented in [M5_ACCEPTANCE.md](file:///D:/projects/ruby-agent/docs/M5_ACCEPTANCE.md).

### Acting Orb Color Palette
- **Initial Implementation**: `#d97706` (generic amber)
- **Proposed Refinement**: Aligned with Ruby's editorial light-mode design tokens using warm bronze amber:
  - Base colors: `["#86560D", "#B45309"]`
  - Highlight colors: `["#F6ECD9", "#E8D5B5"]`
  - Status indicator: `#86560D` on `#F6ECD9`
- **Status**: `needs Haseeb approval` (explicitly tagged for review before promotion).

---

## 2. Milestone M4 Deliverables & Verification Summary

1. **Kill Switch & Input Release**:
   - Esc-hold / global cancellation token cancels in-flight reasoning calls and immediately releases all held keys and mouse buttons.
   - Verified via:
     - `test_kill_switch_aborts_hanging_model_call`
     - `test_kill_switch_during_drag_releases_mouse`
     - `test_kill_switch_during_held_key_releases_key`

2. **Password Guardrails & Browser Verification**:
   - Win32 `ES_PASSWORD` / `EM_GETPASSWORDCHAR` and UI Automation `CurrentIsPassword` checks run before any `TypeText` action.
   - Browser targets (`chrome`, `msedge`, `firefox`, `brave`) strictly fail closed if input element cannot be inspected.
   - Verified via:
     - `test_password_field_detected_refuses_typing`
     - `test_unverifiable_field_in_browser_refuses_typing`

3. **Platform Session & Elevation Protections**:
   - Immediate halt if Windows user session is locked, foreground window is elevated, or secure desktop/UAC is active.
   - Verified via:
     - `test_session_locked_stops_agent_loop`
     - `test_elevated_foreground_or_secure_desktop_stops_agent_loop`

4. **Audit Log Hygiene**:
   - Never records typed text or screenshot payloads in audit logs or step history.
   - Verified via:
     - `test_audit_log_hygiene_never_records_typed_text_or_screenshots`

5. **DPI & Multi-Monitor Geometry**:
   - Per-monitor DPI awareness and unified coordinate space supporting 100%, 125%, 150%, 200% scaling and negative-origin secondary monitors.
   - Verified via:
     - `test_dpi_scaling_100_percent`
     - `test_dpi_scaling_125_percent`
     - `test_dpi_scaling_150_percent`
     - `test_dpi_scaling_200_percent`
     - `test_multi_monitor_negative_origin_secondary`

6. **Gemini Vision Real Model Path & Third-Party Protection**:
   - Primary `gemini-3.8-flash` -> fallback `gemini-3-flash-preview` chain.
   - FreeLLMAPI third-party fallback blocked unless `allow_third_party_fallback = true` AND explicit confirmation that screenshots leave the machine is granted.
   - Verified via:
     - `test_third_party_fallback_blocked_without_explicit_confirmation`
     - `test_third_party_fallback_refused_when_allow_fallback_is_false`

7. **Config Defaults & Telegram Channel Integration**:
   - `computer_use.enabled = false` and `dry_run = true` defaults.
   - Telegram `/do` refuses with an explanatory message when disabled.
   - Consequential action confirmations use shared `UnifiedConfirmationManager` (120s TTL, 5-attempt cap).
   - Verified via:
     - `test_computer_use_config_defaults`
     - `test_telegram_do_refuses_when_computer_use_disabled`
     - `test_m4_consequential_confirmation_uses_unified_confirmation_manager`

8. **User-Configurable Denylist**:
   - Matches process name and window title case-insensitively, preserving built-in defaults (`1Password`, `Bitwarden`, `KeePass`, `Chase`, `regedit`, `taskmgr`, etc.).
   - Verified via:
     - `test_denylist_matches_process_name_case_insensitive`
     - `test_denylist_matches_window_title_case_insensitive`
     - `test_denylist_custom_user_entries_matched`
     - `test_denylist_preserves_built_in_defaults`

---

## 3. Milestone M5 Deliverables: Background Tasks (Mode B Supervisor)

1. **Durable SQLite Queue (`DurableJobStore`)**:
   - Manages background jobs and log records in SQLite (`background_jobs` and `job_logs`).
   - Survived app restarts, supports `Queued`, `Running`, `Verifying`, `Completed`, `Failed`, `Cancelled`, `Timeout`.
   - Verified via:
     - `test_sqlite_job_store_crud_and_status`
     - `test_sqlite_job_store_logs`

2. **ExecutionBackend & Safety (`LocalToolBackend`)**:
   - `GUI_ISOLATION_REQUIRED`: Refuses any interactive GUI/Mode A computer-use attempt by background jobs.
   - `allow_advanced_shell: false` by default: Blocks shell redirection, subshells, and command chains.
   - Environment secret scrubbing: Strips `KEY`, `TOKEN`, `SECRET`, `PASSWORD`, `CREDENTIAL`.
   - Verified via:
     - `test_local_tool_backend_success`
     - `test_local_tool_backend_gui_isolation_refusal`
     - `test_local_tool_backend_advanced_shell_disabled_by_default`

3. **Background Supervisor Worker (`BackgroundTaskSupervisor`)**:
   - Asynchronous worker loop with cancellation tokens and execution timeouts.
   - Verified via:
     - `test_background_task_supervisor_lifecycle`
     - `test_background_task_supervisor_timeout`
     - `test_background_task_supervisor_cancellation`

4. **Multi-Channel Control (Telegram & Console)**:
   - Telegram commands `/jobs`, `/job <id>`, `/cancel <id>`, `/logs <id>`.
   - Desktop Tauri commands (`submit_background_job`, `list_background_jobs`, etc.) and Console UI.
   - Verified via:
     - `test_handle_background_command`
     - `test_telegram_background_job_commands_lifecycle`
     - Frontend build (`npm run build`)

---

## 4. V1 Voice Live Deliverables

1. **Bidirectional Streaming Protocol**:
   - Client and server audio framing, turn completion, rate-limit backoff, and state machines.
   - Verified via:
     - `test_mock_level_voice_live_session_lifecycle`
     - `test_mock_level_rate_limit_backoff`

2. **Safety & Policy Gating**:
   - Tool calls route through `ruby-policy` and `UnifiedConfirmationManager`.
   - Tool results strictly wrapped in `[DATA_OUTPUT]...[/DATA_OUTPUT]`.
   - Hard session limit (default 10 min watchdog).
   - Audio and transcript content never written to logs.
   - Third-party fallback strictly disabled for live audio.
   - Verified via:
     - `test_mock_level_policy_gated_live_tool_dispatcher_data_output_and_confirmation`
     - `test_mock_level_transcript_prompt_injection_sanitization`
     - `test_mock_level_live_session_hard_duration_limit`
     - `test_mock_level_third_party_fallback_refused_for_live_audio`

---

## 5. Negative Invariants & Affirmation

- **G3 Invariant**: Zero physical keyboard keystrokes, mouse moves, mouse clicks, screen captures, or clipboard reads/writes were dispatched or observed on physical hardware during automated test suites.
- All tests executed hermetically using mock executors and memory capture.
- Live hardware tests remain opt-in under `--features live-input`.
