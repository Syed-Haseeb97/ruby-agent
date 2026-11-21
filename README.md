# Ruby — Autonomous Desktop AI Assistant

[![Platform](https://img.shields.io/badge/Platform-Windows%2011-blue.svg)](https://microsoft.com/windows)
[![Rust](https://img.shields.io/badge/Language-Rust%202021-orange.svg)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/GUI-Tauri%20v2%20%2B%20React%2019-teal.svg)](https://tauri.app/)

> An autonomous, privacy-first desktop AI assistant built in Rust and React/Tauri for Windows. Ruby combines real-time neural speech dictation, visual mood feedback, OS-level computer use automation with hardware kill-switches, cryptographically-gated tool execution, and local-first memory.

---

## 🌟 Key Capabilities

- **🎙️ Real-time Speech Dictation (M1)**: Low-level Win32 keyboard hook (`WH_KEYBOARD_LL`), Silero VAD, neural STT (Parakeet / Whisper), elevation & UIPI security checks, transient clipboard injection with Windows Clipboard History exclusion.
- **🔮 Visual Orb & Moods (M0)**: Slint + Canvas shader orb displaying 9 emotional and operational states in warm archival paper aesthetics (`#F5F3EE` canvas, white cards, stone hairlines `#C8C2B5`, zero blinking/pulsing animations).
- **🖥️ Computer Use & OS Automation (M4)**: Windows input synthesis, UIA/OCR text inspection, 1200ms `Esc` emergency kill-switch, password field refusal, and sensitive window denylisting.
- **🛡️ Policy Engine & Tool Gating (`ruby-policy`)**: Enforces strict privilege tiers: `ReadOnly` tool requests are auto-allowed, while `Write`, `Destructive`, or `Unknown` actions are gated by CSPRNG 6-digit confirmation codes. Includes strict SSRF and URL validation. Untrusted model inputs are framed strictly as passive `DATA` blocks.
- **⚡ Durable Background Tasks (M5)**: SQLite-backed persistent job queue (`%LOCALAPPDATA%\Ruby\jobs.db`) with exponential backoff retry state machine, concurrency limiting, and crash-safe restart recovery.
- **📱 Secure Remote Control (R0–R2)**: Long-polling Telegram bot with cryptographic pairing tokens, allowed-roots file isolation, and desktop screenshot privacy protection.
- **🩺 Pre-flight Diagnostics & Redacting Logger**: Structured JSON-Lines logger with daily rotation, 14-day retention pruning, and comprehensive multi-pattern secret scrubbing. CLI and GUI pre-flight health doctor.

---

## 🏗️ Workspace Map & Crate Topology

| Crate | Path | Responsibility |
| :--- | :--- | :--- |
| `ruby-config` | `crates/ruby-config/` | Application configuration defaults, disk path resolution (`%LOCALAPPDATA%\Ruby\`). |
| `ruby-policy` | `crates/ruby-policy/` | File system policy, SSRF/URL validation, tool classification, confirmation manager. |
| `ruby-platform` | `crates/ruby-platform/` | Win32 FFI: keyboard hooks, clipboard injection, elevation check, Job Object process trees, mutex guard. |
| `ruby-gemini` | `crates/ruby-gemini/` | Gemini API client (Flash, Pro, Vision, Live), FreeLLMAPI fallback guard. |
| `ruby-dictation` | `crates/ruby-dictation/` | Speech-to-text pipeline, VAD, synthetic audio cues, diagnostic CLI. |
| `ruby-voice` | `crates/ruby-voice/` | Piper TTS engine, Voice Live session manager, wake word trait. |
| `ruby-actions` | `crates/ruby-actions/` | Allowlisted local actions (file, app, settings, media), screen recorder, Computer Use loop. |
| `ruby-memory` | `crates/ruby-memory/` | SQLite long-term memory store (`ruby_memory.db`) with FTS5 search and secret scanning. |
| `ruby-mcp` | `crates/ruby-mcp/` | Model Context Protocol stdio client, 1 MB bounded buffer parser, Playwright browser integration. |
| `ruby-core` | `crates/ruby-core/` | Background task supervisor, pre-flight doctor diagnostics, structured redacting logger, metrics. |
| `ruby-remote` | `crates/ruby-remote/` | Telegram remote control channel, pairing manager, power/sleep state watcher. |
| `ruby-desktop` | `crates/ruby-desktop/` | Tauri v2 desktop application, React frontend, centralized settings, setup wizard. |

---

## 🚀 Quick Start & Development

### Prerequisites
- **OS**: Windows 10/11 (64-bit)
- **Rust**: `stable` (Rust 2021 edition)
- **Node.js**: `v20+` & npm
- **C++ Build Tools**: MSVC v143+ (via Visual Studio Installer)

### Development Setup

```powershell
# 1. Install frontend dependencies and build assets
cd crates/ruby-desktop/frontend
npm install
npm run build
cd ../../../

# 2. Run diagnostic doctor to verify system prerequisites
cargo run -p ruby-desktop -- doctor

# 3. Launch desktop assistant in debug mode
cargo run -p ruby-desktop
```

### Production Build

```powershell
cargo build --release -p ruby-desktop
# Executable produced at: .\target\release\ruby-desktop.exe
```

### Running Test Suites

```powershell
# Run full workspace test suite
cargo test --workspace

# Run secret scanner across all files
python acceptance/secret_scan.py --all

# Run frontend tests
cd crates/ruby-desktop/frontend && npm test && cd ../../../
```

---

## 🔒 Security Architecture

1. **Prompt Injection Immunity**: Model inputs are framed inside `<DATA>` blocks with strict token guards.
2. **Emergency Esc**: Holding or pressing `Esc` for 1200ms instantly aborts any running Computer Use automation.
3. **Hardware Job Objects**: Spawned child processes are attached to Win32 Job Objects with `KILL_ON_JOB_CLOSE` to prevent orphaned background tasks.
4. **Secret Redaction**: All logs scrub API keys, authorization tokens, passwords, and sensitive paths prior to disk write.
