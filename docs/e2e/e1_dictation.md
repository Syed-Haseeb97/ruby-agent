# E2E Scenario E1: Dictation Flow

**Level**: Mock-Level / Verified in Unit Tests (`crates/ruby-dictation/`)  
**Components**: Global Low-level Keyboard Hook, Silero VAD, Parakeet/Whisper Engine, UIPI Check, Text Injector with Clipboard History Exclusion.

---

## Scenario Execution Transcript

```
[00:00:00.000] [SYSTEM] Ruby Assistant initialized. State = Idle. Listening for hotkey (Caps Lock or user hotkey).
[00:00:01.200] [HOOK] Low-level keyboard hook catches hotkey down event.
[00:00:01.205] [SECURITY] Pre-flight UIPI check: GetForegroundWindow() -> Notepad.exe (PID: 10420).
[00:00:01.210] [SECURITY] Foreground process is NOT elevated. UIPI check passed.
[00:00:01.215] [SECURITY] Window title check: "Untitled - Notepad" does not match security denylist.
[00:00:01.220] [AUDIO] Microphone audio capture begins (16 kHz, 16-bit mono PCM).
[00:00:01.230] [AUDIO_CUE] Play start dictation WAV cue (synthetic 440 Hz tone, 80 ms).
[00:00:01.250] [ORB_UI] Orb state transition -> Listening (Amber/Golden glow, calm breathing shader).
[00:00:02.500] [VAD] Silero VAD detects speech segment: probability = 0.94.
[00:00:03.800] [HOOK] Hotkey released. Audio capture ends (total audio: 2.6s, 83,200 bytes PCM).
[00:00:03.810] [ORB_UI] Orb state transition -> Thinking (Rotating geometric lattice, purple/blue).
[00:00:03.820] [STT] Audio sent to Parakeet STT engine.
[00:00:04.150] [STT] Transcription complete: "Refactor the authentication module to use Argon2id hashing."
[00:00:04.155] [SECURITY] Secret scan on transcribed text: 0 matches for API keys/tokens. Passed.
[00:00:04.160] [TEXT_INJECTION] Register format "ExcludeClipboardContentFromMonitorProcessing".
[00:00:04.162] [TEXT_INJECTION] Snapshot user's existing clipboard contents.
[00:00:04.165] [TEXT_INJECTION] Set transient clipboard with history exclusion flags.
[00:00:04.170] [TEXT_INJECTION] Synthesize Ctrl+V (keybd_event VK_CONTROL down, VK_V down, up, up).
[00:00:04.190] [TEXT_INJECTION] Restore user's original clipboard contents.
[00:00:04.195] [AUDIO_CUE] Play stop dictation WAV cue (synthetic 880 Hz tone, 60 ms).
[00:00:04.200] [ORB_UI] Orb state transition -> Idle (Calm pearl white / stone hairline).
[00:00:04.205] [METRICS] Dictation completed: duration=2.6s, latency=385ms, status=Success.
```

---

## Verification Evidence
- **Automated Tests**: `crates/ruby-dictation/tests/` (15/15 passed).
- **Elevation Check**: Verified in `test_foreground_elevation_guard`.
- **Clipboard Restoration**: Verified in `ruby-platform::text_injection::tests`.
