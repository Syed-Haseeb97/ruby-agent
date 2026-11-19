# Changelog

All notable changes to the Ruby Assistant architecture during Checkpoint 5 Mega Batch.

## [0.1.0] - 2026-11-19 (Checkpoint 5 Mega Batch)

### Added
- **Part 0 (Z1â€“Z4 Fixes)**:
  - Esc-hold kill switch semantics (1200ms) halts all active operations while keeping desktop assistant running with static notice.
  - Standardized CSPRNG 6-digit confirmation codes across all subsystems (`ruby-policy`, `ruby-memory`, `ruby-actions`, `ruby-remote`, `ruby-desktop`).
  - Screen recorder segmentation and sub-500ms denylist polling with automatic segment deletion.
  - Python BOM handling in test summary reporter; CI secret scanner negative verification test.
- **Part 1 (Lifecycle & Hardening: L1â€“L8)**:
  - `SingleInstanceGuard` using Win32 named mutex (`Global\RubyAssistant_SingleInstance_Mutex`).
  - `AutostartManager` managing Windows Registry Run key with dry-run protection.
  - `KeepAwakeManager` calling `SetThreadExecutionState` to prevent system sleep during active tasks.
  - `PowerStatusProvider` inspecting AC battery status and system uptime via Win32 API.
  - `CrashReporter` managing panic hooks with secret redaction and crash dump isolation.
  - `SessionLockWatcher` monitoring Windows workstation lock state (`WM_WTSSESSION_CHANGE`).
  - Telegram error classification with exponential backoff and randomized jitter; `/quit` command.
- **Part 2 (UI Integration & Doctor: U1â€“U5)**:
  - Centralized `SettingsPanel.tsx` in strict light mode (`#F5F3EE` warm archival paper, `#FFFFFF` cards, stone hairlines `#C8C2B5`, Instrument Serif + Geist typography). Zero pulsing/blinking dots.
  - 4-step `SetupWizard.tsx` modal for first-run onboarding.
  - Comprehensive Diagnostic Doctor CLI (`ruby-desktop doctor [--json]`) and UI modal (`DoctorModal.tsx`) inspecting credentials, storage isolation, security flags, external tools, and system resources.
  - Vitest test suite in `crates/ruby-desktop/frontend/src/settings.test.tsx` (5/5 passed).
- **Part 3 (Observability: O1â€“O3)**:
  - Structured JSON-lines rotating logger (`crates/ruby-core/src/logging.rs`) with 14-day retention pruning and universal secret redaction.
  - In-memory telemetry metrics engine (`MetricsCollector`) tracking STT latency, model fallbacks, background jobs, and remote commands.
  - Desktop IPC commands (`get_recent_logs`, `get_system_metrics`) and Telegram commands (`/logs <n>`, `/status`).
- **Part 4 (Security Self-Audit, Fuzzing & Supply Chain: X1â€“X7)**:
  - Complete Threat Model (`docs/THREAT_MODEL.md`) covering STRIDE analysis across 5 trust boundaries.
  - Full Tauri IPC audit table in `docs/evidence/CP5/part4.md` for all 34 commands.
  - Property testing & generative fuzzing: path canonicalization (5,000 runs) and URL/SSRF validation (5,000 runs) in `ruby-policy`, computer action parser in `ruby-actions`, and Telegram parser in `ruby-remote`.
  - Supply chain verification: `npm audit` reporting 0 vulnerabilities; census of all 39 `unsafe` blocks.
  - Zero secrets at rest verified; Key Rotation guide written (`docs/KEY_ROTATION.md`).
  - Prompt injection regression test suite (`crates/ruby-core/tests/prompt_injection_matrix.rs`) with 10/10 test cases passed.
- **Part 5 (M6 Parity)**:
  - M6 inventory verified against legacy Python Ruby baseline; all 16 F-features accounted for and verified.
  - Added safe `LocalAction::MediaControl` allowlist in `ruby-actions`.
- **Part 6 (Packaging & Release: P1â€“P2)**:
  - Production cargo release profile configured (`opt-level = 3`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = true`).
  - Tauri bundle installer metadata configured for Windows NSIS/MSI installers.
  - Production Release Checklist (`docs/RELEASE_CHECKLIST.md`).
  - Automated SPDX 2.3 SBOM generator script (`scripts/generate_sbom.py`).
  - GitHub Actions release pipeline (`.github/workflows/release.yml`).
- **Part 7 (End-to-End Scenarios: E1â€“E6)**:
  - 6 end-to-end integration scenarios written with mock-level evidence in `docs/e2e/`.
- **Part 8 (Final Verification & Docs)**:
  - Project handoff guide (`docs/HANDOFF.md`), known limitations (`docs/KNOWN_LIMITATIONS.md`), and manual verification checklist (`docs/MANUAL_CHECKLIST.md`).
