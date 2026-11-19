# Ruby Assistant — Manual Hardware & Credentials Checklist

This checklist is for the human operator to manually verify Ruby Assistant on real Windows hardware with live audio, live display, and active API credentials.

---

## 1. Pre-Flight Diagnostics
- [ ] **Run Diagnostic Doctor**:
  ```powershell
  cargo run -p ruby-desktop -- doctor
  ```
  - Verify `Storage Isolation: HEALTHY`
  - Verify `Credentials: HEALTHY` (Gemini API Key detected)
  - Verify `Security Defaults: HEALTHY` (All dangerous features OFF)
  - Verify `External Tools: HEALTHY` (git, ffmpeg, node detected)

---

## 2. Visual Orb & Mood Shaders (M0)
- [ ] Launch desktop app: `cargo run -p ruby-desktop`
- [ ] Verify floating Orb appears in top-right or center without window decorations.
- [ ] Verify Orb background is transparent and click-through functions when enabled.
- [ ] Confirm strict light mode aesthetics:
  - Background: Warm archival paper `#F5F3EE`
  - Cards: White `#FFFFFF` with stone hairline borders `#C8C2B5`
  - Typography: Instrument Serif display, Geist body, Geist Mono metadata
  - Status Indicators: Completely static pills with **zero blinking, pulsing, or animated dots**.

---

## 3. Speech Dictation (M1)
- [ ] Open Notepad: `notepad.exe`
- [ ] Press and hold the dictation hotkey (e.g. `Caps Lock` or configured key).
- [ ] Verify start audio cue plays (clean synthetic chime).
- [ ] Verify Orb transitions to **Listening** mood (amber/golden glow).
- [ ] Speak clearly: *"The quick brown fox jumps over the lazy dog."*
- [ ] Release the hotkey.
- [ ] Verify stop audio cue plays.
- [ ] Verify Orb transitions to **Thinking** mood, then returns to **Idle**.
- [ ] Verify transcribed text appears in Notepad.
- [ ] Verify original clipboard content was preserved (paste with Ctrl+V to verify your previous clipboard text).

---

## 4. UIPI & Elevation Guard (M1)
- [ ] Open an elevated PowerShell prompt (*Run as Administrator*).
- [ ] Focus the elevated window and press the dictation hotkey.
- [ ] Verify Ruby refuses injection and displays elevation warning notice rather than crashing or freezing.

---

## 5. Emergency Kill Switch (M4)
- [ ] In Console Settings, temporarily check *Enable Computer Use* (in Dry Run mode).
- [ ] Start a test automation task.
- [ ] Press and hold the `Escape` key for 1.2 seconds (1200 ms).
- [ ] Verify task halts immediately.
- [ ] Verify Desktop Console stays running and displays static notice: `"Emergency kill switch activated. All inputs released."`

---

## 6. Remote Control & Pairing (R0–R2)
- [ ] In Telegram, send `/start` to your Ruby bot.
- [ ] Verify bot replies with a 6-digit challenge code (e.g. `482910`).
- [ ] In Ruby Console -> **Settings** -> **Remote**, enter the 6-digit code.
- [ ] Verify pairing succeeds.
- [ ] Send `/status` in Telegram; verify uptime, power status, and metrics are returned.
- [ ] Send `/read D:\projects\ruby-agent\Cargo.toml`; verify contents returned.
- [ ] Send `/read C:\Windows\System32\drivers\etc\hosts`; verify request is refused (`Path outside allowed roots`).
- [ ] Send `/screen`; verify request is refused by default (`Screen capture disabled`).

---

## 7. Memory & Confirmation Manager (M3 / S6)
- [ ] In Console chat, enter: *"Remember that my favorite programming language is Rust."*
- [ ] Ask: *"What is my favorite programming language?"*; verify Ruby recalls *"Rust"*.
- [ ] Open **Settings** -> **Memories** -> Click **Wipe All Memories**.
- [ ] Verify a confirmation modal appears displaying the exact count of memories and requesting the 6-digit code.
- [ ] Verify entering an invalid code is refused.
- [ ] Enter the correct code; verify all memories are wiped.

---

## 8. Clean Application Exit
- [ ] Send `/quit` in Telegram or click Close in Desktop Console.
- [ ] Verify all background threads, hooks, and child processes exit cleanly.
- [ ] Confirm no orphaned processes remain in Task Manager (`ruby-desktop.exe`).
