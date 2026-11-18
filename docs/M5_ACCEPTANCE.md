# Milestone M5 Acceptance Report: Voice Live & Tool Execution

**Author**: Ruby Assistant Team  
**Date**: 2026-11-18  
**Repository**: `Syed-Haseeb97/ruby-agent`  
**Milestone**: M5 (Voice Live & Tool Execution)

---

## 1. Executive Summary

Milestone M5 delivers Ruby's **Voice Live Pipeline**: a low-latency, bidirectional real-time audio interaction engine powered by Google Gemini Live API (`gemini-3.8-live`). The system enables hands-free spoken dialogue with natural interruption (VAD), synchronized Liquid Orb operational state transitions, real-time transcript streaming, and direct model-initiated tool execution via Ruby's policy-gated MCP infrastructure.

All components adhere strictly to project invariants: zero unobserved live audio devices in automated CI, fail-closed policy gating on tool calls, editorial light-mode aesthetics (warm archival paper `#F5F3EE`, stone hairline `#C8C2B5`), and static status capsules with ZERO pulsing or blinking dots.

---

## 2. Architecture & Deliverables

### A. Gemini Live API Protocol & Session Handle (`ruby-gemini::live_session`)
- **Protocol Framing**: Strongly-typed structures for Gemini Live WebSocket wire protocol (`LiveClientMessage`, `LiveSetupPayload`, `RealtimeInput`, `LiveToolResponse`).
- **Event Streaming**: Bidirectional `LiveEvent` stream dispatching `AudioDelta` (24kHz 16-bit LE PCM), `Transcript` text, `ToolCall` requests, `TurnComplete` markers, and `Interrupted` signals.
- **Hermetic Mock Session**: `MockLiveSession` allows offline integration tests without requiring live internet connections or network credentials.
- **Server Message Parser**: `real::parse_server_message` verifies JSON deserialization for all server event types.

### B. Voice Live Session Manager (`ruby-core::voice_live`)
- **Audio Output Abstraction**: `PcmAudioOutput` trait with `RecordingAudioOutput` for test capture and `NullAudioOutput` for silent operation.
- **Tool Dispatcher Abstraction**: `LiveToolDispatcher` trait allowing Live API function calls to be evaluated safely.
- **Async Cancellation & Loop Lifecycle**: Non-blocking `tokio::select!` event loop that handles user cancellation, session close, and clean transition between `Idle`, `Connecting`, `Active`, and `Stopping` states.
- **Server Interruption Handling**: Automatically interrupts local PCM playback upon receiving an `Interrupted` event when user speaks over the assistant.
- **Orb State Synchronization**: Drives Orb states (`Listening`, `Speaking`, `Thinking`, `Idle`) matching the exact phase of the live conversation.

### C. Desktop Integration & Tauri IPC (`ruby-desktop::commands`)
- **Tauri IPC Commands**:
  - `start_voice_live`: Launches background Voice Live pipeline.
  - `stop_voice_live`: Triggers clean cancellation and input flushing.
  - `get_voice_live_status`: Reports session state (`idle`, `connecting`, `active`, `stopping`).
  - `get_voice_live_transcript`: Retrieves all transcribed turns with timestamps.
  - `push_voice_live_audio`: Streams base64-encoded PCM audio from frontend microphone into the session.

### D. Editorial Console UI (`ConsoleView.tsx`)
- **Live Voice Section**: Elegant editorial interface with live status capsules, Aoede voice badge, and real-time transcript display.
- **Zero Blinking Dots**: Strictly calm, static status indicators in accordance with user global preferences.
- **Escape Integration**: Global `Escape` key immediately terminates active Live voice sessions.

---

## 3. Verification Evidence Matrix

| Item | Description | Level | Evidence / Named Test |
|---|---|---|---|
| **C1.1** | Live API config default model (`gemini-3.8-live`, voice: Aoede) | **VERIFIED** | `ruby-gemini::live_session::tests::test_live_session_config_default_model` |
| **C1.2** | Mock Live session emits `SetupComplete` on initialization | **VERIFIED** | `ruby-gemini::live_session::tests::test_mock_live_session_emits_setup_complete` |
| **C1.3** | Mock Live session echoes synthetic audio and transcript on non-empty chunk | **VERIFIED** | `ruby-gemini::live_session::tests::test_mock_live_session_responds_to_audio_chunk` |
| **C1.4** | Mock Live session ignores empty audio payloads | **VERIFIED** | `ruby-gemini::live_session::tests::test_mock_session_ignores_empty_audio` |
| **C1.5** | Mock Live session close command ends channel cleanly | **VERIFIED** | `ruby-gemini::live_session::tests::test_mock_live_session_close_ends_channel` |
| **C1.6** | Mock Live session cancel token activates on close | **VERIFIED** | `ruby-gemini::live_session::tests::test_mock_live_session_cancel_token_set_on_close` |
| **C1.7** | Server message parsing: `SetupComplete` | **VERIFIED** | `ruby-gemini::live_session::tests::test_server_message_parsing_setup_complete` |
| **C1.8** | Server message parsing: `TurnComplete` | **VERIFIED** | `ruby-gemini::live_session::tests::test_server_message_parsing_turn_complete` |
| **C1.9** | Server message parsing: `ToolCall` | **VERIFIED** | `ruby-gemini::live_session::tests::test_server_message_parsing_tool_call` |
| **C1.10** | Server message parsing: `Interrupted` | **VERIFIED** | `ruby-gemini::live_session::tests::test_server_message_parsing_interrupted` |
| **C2.1** | Voice Live manager starts in Idle state | **VERIFIED** | `ruby-core::voice_live::tests::test_voice_live_starts_as_idle` |
| **C2.2** | Voice Live manager start transitions to Active and emits Listening | **VERIFIED** | `ruby-core::voice_live::tests::test_voice_live_start_transitions_to_active` |
| **C2.3** | Voice Live manager rejects duplicate start when already active | **VERIFIED** | `ruby-core::voice_live::tests::test_voice_live_rejects_double_start` |
| **C2.4** | Audio response received from model is played via `PcmAudioOutput` | **VERIFIED** | `ruby-core::voice_live::tests::test_voice_live_audio_response_played` |
| **C2.5** | Transcripts from model turns are accumulated in transcript history | **VERIFIED** | `ruby-core::voice_live::tests::test_voice_live_transcript_recorded` |
| **C2.6** | Stop command cleanly transitions manager back to Idle state | **VERIFIED** | `ruby-core::voice_live::tests::test_voice_live_stop_transitions_to_idle` |
| **C2.7** | Model-initiated tool calls are routed to `LiveToolDispatcher` | **VERIFIED** | `ruby-core::voice_live::tests::test_voice_live_tool_dispatch_called` |
| **C2.8** | Interrupted event halts active playback via `audio_output.interrupt()` | **VERIFIED** | `ruby-core::voice_live::tests::test_voice_live_interrupted_calls_audio_output_interrupt` |
| **C2.9** | Mic audio push before session start returns `VoiceLiveError::NotStarted` | **VERIFIED** | `ruby-core::voice_live::tests::test_push_mic_audio_fails_when_not_started` |
| **C2.10** | Orb emits `Thinking` operational state during tool execution | **VERIFIED** | `ruby-core::voice_live::tests::test_orb_emits_thinking_during_tool_dispatch` |
| **C3.1** | Desktop IPC: `start_voice_live` and `stop_voice_live` commands | **VERIFIED** | `ruby-desktop::commands::voice_live_tests::test_voice_live_command_start_and_stop` |
| **C3.2** | Desktop IPC: double start returns error | **VERIFIED** | `ruby-desktop::commands::voice_live_tests::test_voice_live_double_start_rejected` |
| **C3.3** | Desktop IPC: `push_voice_live_audio` before start fails gracefully | **VERIFIED** | `ruby-desktop::commands::voice_live_tests::test_push_mic_audio_before_start_errors` |
| **C3.4** | Desktop IPC: `get_voice_live_status` returns correct idle state string | **VERIFIED** | `ruby-desktop::commands::voice_live_tests::test_get_voice_live_status_idle_string` |
| **C4.1** | Console UI Voice Live Pipeline section & transcript feed | `compiles-only` | Implemented in `ConsoleView.tsx`; verified via `npm run build` |
| **C4.2** | Frontend build verification | **VERIFIED** | `npm run build` (vite v6.4.6, 36 modules, dist/index.html 0.96kB) |

---

## 4. Test Summary Across Workspace

- **Total Workspace Tests Passing**: 212 passed; 0 failed; 0 ignored
  - `ruby-actions`: 36 passed
  - `ruby-config`: 2 passed
  - `ruby-core`: 42 passed
  - `ruby-desktop`: 7 passed
  - `ruby-dictation`: 15 passed
  - `ruby-gemini`: 29 passed
  - `ruby-mcp`: 7 passed
  - `ruby-memory`: 7 passed
  - `ruby-platform`: 12 passed
  - `ruby-policy`: 24 passed
  - `ruby-remote`: 25 passed
  - `ruby-voice`: 6 passed
- **Linter & Formatting**: `cargo clippy --workspace` clean, `npm run build` clean.
- **Rule G4 Adherence**: `keyboard_hook.rs`, `text_injection.rs` (runtime), `elevation_check.rs` (runtime), and `dictation_engine.rs` remain completely untouched.
