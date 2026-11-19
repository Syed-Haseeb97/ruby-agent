# Part 3 — Observability & Diagnostics (O1–O3) Evidence

**Status**: COMPLETED
**Date**: 2026-11-19
**Scope**: Structured Tracing & Daily Rotating Redacted Logs (O1), In-Memory Metrics & Telemetry (O2), Redaction Property & Fuzz Tests (O3).

---

## 1. O1: Structured Tracing & Daily-Rotating Redacted Logs

- **Implementation**: `crates/ruby-core/src/logging.rs`, integrated into `AppState` (`crates/ruby-desktop/src/lib.rs`), IPC commands (`crates/ruby-desktop/src/commands.rs`), and Telegram adapter (`crates/ruby-remote/src/telegram/mod.rs`).
- **Features**:
  - Daily-rotating JSON-lines file appender in `%LOCALAPPDATA%\Ruby\logs\ruby-YYYY-MM-DD.log`.
  - 14-day retention: automatically prunes files matching `ruby-*.log` older than 14 days on startup.
  - Global redaction layer: regex matching for all S10 secret classes (Google API Key `AIza...`, OpenAI/Anthropic `sk-...`, FreeLLMAPI `freellmapi-...`, Telegram bot tokens, GitHub PAT `ghp_...` and fine-grained `github_pat_...`, Private Key headers, AWS access keys `AKIA...`, and tagged `[SECRET]...[/SECRET]`). All replaced with `[REDACTED]` prior to disk writes.
  - Ring buffer of last 500 entries exposed via Console IPC `get_recent_logs` and Telegram `/logs <n>`.

---

## 2. O2: In-Memory Metrics Collector

- **Implementation**: `crates/ruby-core/src/metrics.rs`.
- **Telemetry Captured**:
  - **Dictation Latency Breakdown**: `audio_len_ms`, `upload_ms`, `inference_ms`, `typing_ms`, `total_ms`.
  - **Model Fallbacks**: Counters partitioned by reason (`timeout`, `rate_limit` (429), `server_error`).
  - **Background Jobs**: `active`, `completed`, `failed`, `total_duration_ms`.
  - **Computer Use**: `actions_total`, `actions_confirmed`, `actions_denied`.
  - **Remote Control**: `commands_total`, `active_sessions`.
- **Surfaced Via**:
  - Console IPC: `get_system_metrics` returning typed `SystemMetricsSnapshot`.
  - Telegram `/status`: Enriched with formatted telemetry summary (`format_status_report()`).

---

## 3. O3: Redaction Property & Fuzz Tests

- **Unit & Property Tests**:
  - `crates/ruby-core/src/logging.rs`:
    - `test_redaction_patterns_replace_all_s10_secrets`: Proves every individual S10 pattern is sanitized.
    - `test_daily_rotating_log_file_and_retention_pruning`: Verifies daily log file creation and pruning of files > 14 days old.
    - `test_randomized_property_redaction_fuzzing`: 100 randomized iterations of noise prefix/suffix + secret tokens, asserting secrets are stripped and harmless text preserved.
    - `test_property_fuzzing_all_secret_classes_never_leak_to_disk`: Generates planted secrets across all 9 classes, logs them to disk, reads raw disk file, and verifies 0 secrets leak.
  - `crates/ruby-remote/tests/mock_telegram_tests.rs`:
    - `test_telegram_logs_command_redaction_and_metrics_status`: Verifies Telegram `/status` contains live metrics and `/logs` strips planted secrets over the wire.
  - `crates/ruby-desktop/src/commands.rs`:
    - `test_observability_redacted_logger_and_metrics_ipc`: Verifies desktop IPC exposes sanitized logs and metrics snapshots.
