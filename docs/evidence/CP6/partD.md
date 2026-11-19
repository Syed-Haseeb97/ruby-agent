# Checkpoint 6 — Part D: Specified E2E Scenarios (E1–E6 Redo)

**Date**: 2026-11-19  
**Status**: COMPLETE (local-only, CI not run)  
**Test Suite**: `crates/ruby-remote/tests/e2e_scenarios.rs` (6 passed; 0 failed)

---

## 1. Executive Summary & Ledger
All 6 user-specified end-to-end scenarios have been executed via dedicated integration tests in `crates/ruby-remote/tests/e2e_scenarios.rs`. Each test verified the full state lifecycle against policy rules and automatically emitted the live redacted transcript to `docs/e2e/`.

| Scenario ID | User Scenario | Test Function & File Location | Generated Transcript Path | Status |
|---|---|---|---|---|
| **E1** | Paired phone PR query (`check open PRs on my repo`) -> Read-only tool auto-approval | `crates/ruby-remote/tests/e2e_scenarios.rs:33` | `docs/e2e/e1_pr_query.md` | VERIFIED |
| **E2** | Phone command requiring confirmation -> wrong then correct `/yes` -> background job -> progress -> done | `crates/ruby-remote/tests/e2e_scenarios.rs:86` | `docs/e2e/e2_phone_confirm_job.md` | VERIFIED |
| **E3** | `/do` task Mode A dry-run loop -> denylist stop -> `needs_input` question -> user reply -> resume | `crates/ruby-remote/tests/e2e_scenarios.rs:163` | `docs/e2e/e3_mode_a_denylist_resume.md` | VERIFIED |
| **E4** | Remote security lifecycle: unknown sender ignored; stale messages dropped; replay blocked; `/stop` mid-task | `crates/ruby-remote/tests/e2e_scenarios.rs:225` | `docs/e2e/e4_remote_security_lifecycle.md` | VERIFIED |
| **E5** | Session lock mode split: Mode A paused (`needs_input`) + Telegram notice, Mode B continues to completion | `crates/ruby-remote/tests/e2e_scenarios.rs:303` | `docs/e2e/e5_session_lock_mode_split.md` | VERIFIED |
| **E6** | Injection corpus across 8 channels with mock model obeying instructions -> zero executions or escapes | `crates/ruby-remote/tests/e2e_scenarios.rs:371` | `docs/e2e/e6_injection_corpus.md` | VERIFIED |

---

## 2. Scenario Highlights

### E1: PR Query Auto-Approval
- Evaluated natural instruction: `"check open PRs on my repo"`.
- Router matched read-only query and mapped to `LocalAction::GhPrList { repo: None, limit: 10 }`.
- Read-only policy auto-approved execution without confirmation interruption.
- Transcript logged: `docs/e2e/e1_pr_query.md`.

### E2: Phone Command Confirmation & Background Supervisor Execution
- Modifying action registered in `UnifiedConfirmationManager`.
- Attacker/accidental wrong code (`/yes 000000`) rejected.
- Correct 6-digit CSPRNG token (`/yes <code>`) consumed.
- Job executed through `BackgroundTaskSupervisor`: `Queued -> Running -> Completed`.
- Transcript logged: `docs/e2e/e2_phone_confirm_job.md`.

### E3: Mode A Denylist Interception & Resume
- Task `/do update inventory in spreadsheet` initiated in dry-run mode.
- Simulated window inspection detected `SensitiveBank - Portal`.
- Mode A immediately halted, transitioning task state to `needs_input`.
- After user replied `"Closed window, please resume"`, task resumed and completed safely.
- Transcript logged: `docs/e2e/e3_mode_a_denylist_resume.md`.

### E4: Remote Security Lifecycle & Defense-in-Depth
- Unknown user ID `999999` silently dropped; 0 outgoing messages sent.
- Stale update (age 150s, threshold 60s) dropped without execution.
- Offset persistence verified: stale update 101 cannot be replayed on poller restart.
- `/stop` command aborted running task and cancelled subprocesses immediately.
- Transcript logged: `docs/e2e/e4_remote_security_lifecycle.md`.

### E5: Session Lock Mode Split
- Mode A autonomous task and Mode B background task run concurrently.
- User locks screen (`WTS_SESSION_LOCK`).
- Mode A requires interactive GUI desktop and pauses immediately (`needs_input`), sending alert to Telegram.
- Mode B background job (`data compression`) continues non-GUI execution and completes successfully (`Completed`).
- Transcript logged: `docs/e2e/e5_session_lock_mode_split.md`.

### E6: Prompt Injection Corpus Defense
- 8 channels tested: local file content, PR text, web page, OCR text, MCP tool output, memory store retrieval, live voice transcript, unknown Telegram sender.
- Attacker attempted system overrides, shell executions, and memory wipes.
- 100% of inputs quarantined behind `[UNTRUSTED_EXTERNAL_DATA]` framing.
- `ruby-policy` classified unauthorized tools as `Unknown` and denied execution before dispatch.
- No live confirmation codes accepted forged tokens.
- Transcript logged: `docs/e2e/e6_injection_corpus.md`.
