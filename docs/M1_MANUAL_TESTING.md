# Milestone M1: Wispr Flow-Style Dictation Manual Verification Guide

This document provides step-by-step verification instructions for testing the Ruby Wispr Flow-style system-wide dictation on Windows 11 with physical hardware (keyboard, microphone, monitor).

---

## 1. Prerequisites & API Key Setup

1. **Set Gemini API Key**:
   Set your key in Windows Credential Manager or via environment variable:
   ```cmd
   set GEMINI_API_KEY=your_gemini_api_key_here
   ```
   *(Or store it permanently in Windows Credential Manager under service `"ruby-agent"` and username `"gemini-api-key"`)*.

2. **Launch Ruby in Diagnostic Mode**:
   ```cmd
   cd D:\projects\ruby-agent
   cargo run -p ruby-desktop -- --diagnostic
   ```
   *Expected*: The transparent Liquid Orb appears on screen in the resting `Idle` state. The console logs hook initialization and VAD parameters.

---

## 2. Test Scenarios

### Scenario 1: Basic Hold-to-Talk in Notepad (Short Text < 64 Chars)
1. Open **Notepad** and place the cursor in the text area.
2. Press and hold `Ctrl + Win`.
3. *Expected visual*: After ~250 ms, the Liquid Orb transitions to `Listening` (cyan gradient with soft horizontal gaze drift).
4. Speak: *"Hello world this is a quick test."*
5. Release `Win` or `Ctrl`.
6. *Expected visual*: The Orb transitions to `Thinking` (faceless glossy spinning sphere with specular meridian shading).
7. *Expected output*: The text `"Hello world, this is a quick test."` is typed at the cursor via `SendInput` Unicode. The Orb returns to `Idle`.

### Scenario 2: Long Dictation in VS Code (Clipboard Paste >= 64 Chars)
1. Copy some arbitrary text (e.g., `"MY_PREVIOUS_CLIPBOARD_TEXT"`) to your clipboard.
2. Focus an editor window in **VS Code**.
3. Press and hold `Ctrl + Win`.
4. Speak a multi-sentence thought with disfluencies and self-correction:
   *"Um I want to implement the new handler, no wait, the new parser for json tokens and serialize them into the database."*
5. Release the hotkey.
6. *Expected output*:
   - Text inserted via fast clipboard paste (`Ctrl+V`):
     `"I want to implement the new parser for JSON tokens and serialize them into the database."`
   - Notice: filler `"Um"` was removed, self-correction was resolved to `"the new parser"`, and `JSON` was properly capitalized according to the active IDE context (`code.exe`).
7. Press `Ctrl + V` in Notepad: verify that `"MY_PREVIOUS_CLIPBOARD_TEXT"` is still your clipboard contents (clipboard was cleanly restored).

### Scenario 3: Hands-Free Dictation (Double-Tap Mode)
1. Quickly double-tap `Ctrl + Win` within 350 ms.
2. *Expected visual*: The Orb enters and remains in `Listening` mode without holding any keys.
3. Speak at a comfortable pace.
4. Tap `Ctrl + Win` once (or hit Enter).
5. *Expected visual & output*: The Orb switches to `Thinking`, cleans up and inserts your dictation, and returns to `Idle`.

### Scenario 4: Escape Key Cancellation
1. Press and hold `Ctrl + Win`.
2. While speaking, press `Esc`.
3. *Expected*: Dictation is immediately cancelled, audio buffer discarded, zero text is inserted, and the Orb returns immediately to `Idle`.

### Scenario 5: Third-Key Abort
1. Press and hold `Ctrl + Win`.
2. While holding, press an unrelated third key (e.g. `Tab` or `D`).
3. *Expected*: Hold-to-talk is aborted to allow normal Windows shortcuts to pass through unimpeded.

### Scenario 6: Windows Start Menu Suppression
1. Press and hold `Ctrl + Win` for 1 second without speaking, then release.
2. *Expected*: The Windows Start Menu does **NOT** pop up. The unassigned `VK_NONAME` (`0xFC`) injection successfully suppresses the Windows shell Start Menu trigger.

### Scenario 7: Password Field Protection Guardrail
1. Open Windows Settings, browser login page, or any dialog containing a masked password field (`***`).
2. Focus the password field.
3. Press and hold `Ctrl + Win`, speak, and release.
4. *Expected*: Insertion is refused. The Orb signals `Error` (ruby red with rim distortion) and refuses to type into the password box.

### Scenario 8: Elevated Window UIPI Guardrail
1. Open an Administrator Command Prompt or PowerShell (`Run as Administrator`).
2. Run Ruby as a regular unelevated user.
3. Focus the elevated command prompt and hold `Ctrl + Win` to dictate.
4. *Expected*: UIPI boundary is detected. The dictation refuses injection to prevent UIPI silent drop, and logs a clear warning.

### Scenario 9: Emergency Kill Switch
1. At any point while Ruby is running, press `Ctrl + Alt + Shift + R`.
2. *Expected*: The application and hook threads terminate immediately.

---

## 3. Telemetry & Latency Budget Check
In the diagnostic terminal output, observe the structured log:
```
Dictation pipeline completed successfully vad_ms=42 encode_ms=8 model_ms=840 insert_ms=18 total_ms=908 within_target=true
```
Target latency budget: $p50 < 2000 \text{ ms}$ for a 5-second audio clip.
