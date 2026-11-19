# Ruby Assistant — Project Handoff & Developer Guide

## 1. Architectural Overview

Ruby Assistant is an autonomous, privacy-first desktop AI assistant built in Rust and React/Tauri for Windows. It provides:
- **Speech Dictation (M1)**: Low-level keyboard hook (`WH_KEYBOARD_LL`), Silero VAD, Parakeet/Whisper neural STT, elevation/UIPI check, transient clipboard injection with history exclusion.
- **Visual Orb & Moods (M0)**: Slint + Canvas shader orb displaying 9 mood states in strict light mode (`#F5F3EE` warm archival paper, white cards, stone hairlines `#C8C2B5`, zero blinking/pulsing animations).
- **Computer Use / OS Automation (M4)**: Windows input synthesis, UIA/OCR text inspection, 1200ms Esc emergency kill switch, password field refusal, and sensitive window denylisting.
- **Policy Engine & Model Gating (`ruby-policy`)**: Classifies all model tool requests (`ReadOnly` -> auto-allowed, `Write`/`Destructive`/`Unknown` -> gated by CSPRNG 6-digit confirmation codes). Untrusted inputs are framed strictly as passive `DATA` blocks.
- **Background Tasks (M5)**: Durable SQLite queue (`%LOCALAPPDATA%\Ruby\jobs.db`) with retry state machine, concurrency limiting, and restart recovery.
- **Remote Control (R0â€“R2)**: Long-polling Telegram bot with R0 cryptographically secure pairing, R1 allowed roots file isolation, and R2 full-desktop screenshot protection.
- **Observability & Diagnostics (Doctor / Logging)**: Structured JSON-lines logger with daily file rotation, 14-day retention pruning, and comprehensive secret scrubbing. CLI and GUI pre-flight diagnostics.

---

## 2. Workspace Map & Crate Topology

| Crate | Path | Responsibility |
| :--- | :--- | :--- |
| `ruby-config` | `crates/ruby-config/` | Application configuration defaults, disk path resolution (`%LOCALAPPDATA%\Ruby\`). |
| `ruby-policy` | `crates/ruby-policy/` | File system policy, SSRF/URL validation, tool classification, unified confirmation manager. |
| `ruby-platform` | `crates/ruby-platform/` | Win32 FFI: keyboard hooks, clipboard injection, elevation check, Job Object process trees, mutex guard. |
| `ruby-gemini` | `crates/ruby-gemini/` | Gemini API client (2.5/3.0 Flash, Pro, Vision, Live), FreeLLMAPI fallback guard. |
| `ruby-dictation` | `crates/ruby-dictation/` | Speech-to-text pipeline, VAD, synthetic audio cues, diagnostic CLI. |
| `ruby-voice` | `crates/ruby-voice/` | Piper TTS engine, Voice Live session manager, wake word trait + disabled stub. |
| `ruby-actions` | `crates/ruby-actions/` | Allowlisted local actions (file, app, settings, media), screen recorder, Computer Use loop. |
| `ruby-memory` | `crates/ruby-memory/` | SQLite long-term memory store (`ruby_memory.db`) with FTS5 search and secret scanning. |
| `ruby-mcp` | `crates/ruby-mcp/` | Model Context Protocol stdio client, 1 MB bounded buffer parser, Playwright browser integration. |
| `ruby-core` | `crates/ruby-core/` | Background task supervisor, pre-flight doctor diagnostics, structured redacting logger, metrics. |
| `ruby-remote` | `crates/ruby-remote/` | Telegram remote control channel, pairing manager, power/sleep state watcher. |
| `ruby-desktop` | `crates/ruby-desktop/` | Tauri v2 desktop application, React frontend, centralized settings, setup wizard. |

---

## 3. Real vs Mock Implementation Truth

| Subsystem | Real Implementation | Mock / Stub Implementation |
| :--- | :--- | :--- |
| **Keyboard Hooks** | Real Win32 `SetWindowsHookExW` (`WH_KEYBOARD_LL`) | Trait-abstracted test fakes for unit testing |
| **Elevation Check** | Real Win32 `GetTokenInformation` with UAC token queries | Mock token provider for simulated non-elevated tests |
| **Job Object Tree Kill** | Real Win32 Job Object API (`CreateJobObjectW`, `AssignProcessToJobObject`) | Simulated timeout runner in tests |
| **SQLite Storage** | Real SQLite FTS5 database on disk via `rusqlite` | `open_in_memory()` for isolated unit tests |
| **Secret Redaction** | Real regex-based multi-pattern scrubber across all S10 secrets | Planted test fixtures for regression verification |
| **Screen Recorder** | Real FFmpeg process launcher with `-an` audio-off argument | Fake sleeping mock executable verifying tree kill |
| **Voice Live Audio** | Real bidirectional Gemini Live WebSocket protocol | Mock WebSocket server verifying confirmation gating |
| **Wake Word** | Trait architecture (`WakeWordDetector`) | `DisabledWakeWordDetector` active by default (Default OFF) |
| **Telegram Remote** | Real Telegram Bot API HTTP polling client | `MockTelegramServer` with synthetic message loop |

---

## 4. How to Build & Run

### Debug Mode (Development)
```powershell
# 1. Install frontend dependencies
cd crates/ruby-desktop/frontend
npm install
npm run build
cd ../../..

# 2. Run diagnostic doctor
cargo run -p ruby-desktop -- doctor

# 3. Launch desktop assistant in debug mode
cargo run -p ruby-desktop
```

### Release Mode (Production)
```powershell
cargo build --release -p ruby-desktop
.\target\release\ruby-desktop.exe
```

### Running Test Suites
```powershell
# Run full workspace test suite
cargo test --workspace

# Run secret scanner across all files
python acceptance/secret_scan.py --all

# Run frontend tests
cd crates/ruby-desktop/frontend && npm test && cd ../../..
```
