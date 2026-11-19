# M6 Parity Analysis & Python Ruby Feature Inventory
**Document Path**: `docs/M6_PARITY.md`  
**Reference Repo**: `Syed-Haseeb97/ai_agent` (Commit 7bc2c3a and legacy Python baseline)  
**Status**: INVENTORY & RISK RATING  

---

## 1. Feature Inventory (F1–F16)

The original Python Ruby codebase implemented 16 major functional features ("F-features"). The table below documents their current status in the Rust architecture, security/platform risk rating, and recommended porting order.

| ID | Feature Name | Original Python Implementation | Rust Status | Risk Rating | Recommended Porting Order |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **F1** | **LiquidBlob UI & Moods** | PyQt5 + QPainter bezier math (`ui/liquid_blob.py`) | **Ported (M0)**<br>Slint + wgpu shader | Low | Completed (M0 baseline) |
| **F2** | **Speech-to-Text Dictation** | Whisper / local PyAudio (`voice/stt.py`) | **Ported (M1)**<br>Silero VAD + Parakeet/Whisper | High (audio/privilege) | Completed (M1 with UIPI & elevation guards) |
| **F3** | **Text-to-Speech (TTS)** | Piper TTS / EdgeTTS subprocess | **Ported (M2)**<br>Piper WASM / fallback chain | Medium (subprocess) | Completed (M2) |
| **F4** | **LLM Chat & Orchestration** | Ollama / Gemini API Python SDK | **Ported (M3)**<br>`ruby-gemini` + chat session | Medium (prompt injection) | Completed (M3 with passive DATA blocks) |
| **F5** | **Screen Recording** | FFmpeg CLI subprocess recording desktop | **Implementing (M6/F3)**<br>Explicit confirmation, auto-pause | High (privacy/secret leak) | **Phase 1 Priority** (Video-only, denylist auto-pause) |
| **F6** | **Computer Use / OS Automation**| PyAutoGUI + win32gui | **Ported (M4)**<br>SendInput + UIA + kill switch | Critical (remote execution) | Completed (M4 with kill switch, password guard, denylist) |
| **F7** | **Wake Word Detection** | openWakeWord / Porcupine Python | **Trait + disabled stub (M6/F3)**<br>Default OFF | Medium (audio eavesdropping) | **Phase 1 Priority** (Trait + disabled stub, disabled by default) |
| **F8** | **Remote Control (Telegram)** | `python-telegram-bot` polling | **Ported (R1/R2)**<br>`ruby-remote` + pairing + CSPRNG | High (external commands) | Completed (R1/R2 with pairing & confirmation) |
| **F9** | **MCP Client Extensibility** | Python `mcp` stdio client | **Ported (M3/F4)**<br>`ruby-mcp` + 1MB bounded read | High (untrusted servers) | Completed (M3/F4 with crash isolation & line caps) |
| **F10** | **Long-term Memory** | SQLite + raw table queries | **Ported (M3)**<br>`ruby-memory` + sqlite + secret scan | High (credential retention) | Completed (M3 with secret rejection) |
| **F11** | **Headless Browser Use** | Playwright Python (`playwright`) | **Ported (M3)**<br>Pinned @0.0.84 sidecar | High (SSRF/data exfil) | Completed (M3 with SSRF & isolated profile) |
| **F12** | **Music & Media Playback** | Pygame mixer / sounddevice | **Pending (M6)** | Low | Phase 3 (Media convenience) |
| **F13** | **System & App Launching** | `os.system` / `subprocess.Popen` | **Ported (M5/actions)**<br>Argv-only + allowlist + JobObject | High (command injection) | Completed (M5 allowlist backend) |
| **F14** | **Clipboard Smart Paste** | `pyperclip` / `win32clipboard` | **Ported (ruby-platform)** | High (credential snooping) | Completed (platform clipboard) |
| **F15** | **Calendar & Reminder Tasks** | Local JSON reminders loop | **Ported (M5)**<br>Durable SQLite task queue | Low | Completed (M5 background tasks) |
| **F16** | **Audio Cues & Sound Effects**| `winsound.PlaySound` | **Ported (ruby-dictation)**<br>Synthesized WAV cues | Low | Completed (dictation audio cues) |

---

## 2. Safety Architecture for M6 Deliverables

### A. Wake Word Detection (F7: Trait + Disabled Stub)
- **Trait-Based Architecture**: Behind `WakeWordDetector` trait in `ruby-voice`.
- **Default State**: `DisabledWakeWordDetector` (disabled stub) is active by default. Wake word engine never runs or captures audio without explicit configuration opt-in.
- **Fail-Closed**: Microphone stream is never held open silently in the background.

### B. Video Screen Recording (F5)
- **Explicit Confirmation**: Recording cannot be started without explicit confirmation (Console button or Telegram confirmation code).
- **Video-Only**: No microphone or system audio capture stream to eliminate acoustic surveillance risk.
- **Output Directory**: Saved strictly outside repository root (defaults to `%LOCALAPPDATA%\Ruby\recordings\`).
- **Active Denylist Monitoring**: Screen recorder enumerates visible windows periodically. If any window matching the security denylist (passwords, vaults, financial portals) becomes visible, recording automatically pauses or stops immediately.
- **Static Status Indicator**: UI and console display a clean, non-pulsing, static indicator while recording is active.
