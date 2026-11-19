# Ruby Assistant - First Run Guide & Operator Runbook

This runbook guides operators through building, launching, configuring, and manually validating the Ruby Rust assistant across all operational milestones (M0 through R2).

---

## 1. Prerequisites & Environment Setup

### System Requirements
- **Operating System**: Windows 10 / 11 (64-bit)
- **Rust Toolchain**: Stable 1.80+ (`x86_64-pc-windows-msvc`) with `rustfmt` and `clippy`
- **Node.js**: v20 or v22 LTS with `npm`
- **C++ Build Tools**: Visual Studio 2022 C++ Build Tools (Windows SDK 10.0+)
- **Audio Hardware**: Working microphone and speaker endpoints accessible via Windows WASAPI / CoreAudio (`cpal`)

### Environment Variables
Set the following environment variables in your Windows User or System Environment (or supply them per terminal session). **Do not hardcode secrets into repository files.**

| Variable Name | Purpose | Required / Optional | Default Fallback |
| :--- | :--- | :--- | :--- |
| `GEMINI_API_KEY` | Primary API key for Gemini 2.0/3.8 text, vision, and live streaming | **Required** for AI features | None |
| `TELEGRAM_BOT_TOKEN` | Bot token provided by `@BotFather` for remote control | Optional (needed for R0-R2) | Remote disabled |
| `FREELLMAPI_KEY` | Optional third-party fallback provider key | Optional (strictly policy-gated) | None |
| `RUBY_DATA_DIR` | Custom directory for databases, recordings, logs, and profiles | Optional | `%LOCALAPPDATA%\Ruby\` |

---

## 2. Building Ruby

All builds, dependencies, and artifacts reside strictly outside Drive C:
```powershell
Set-Location -Path "D:\projects\ruby-agent"

# 1. Build frontend bundle
cd crates\ruby-desktop\frontend
npm ci
npm run build
cd ..\..\..

# 2. Build debug binary
cargo build

# 3. Build optimized release binary
cargo build --release
```

---

## 3. Configuration & Safe Defaults

Configuration files are loaded from `%LOCALAPPDATA%\Ruby\` (or `RUBY_DATA_DIR`). All potentially dangerous capabilities are **hard-disabled by default**:

```json
{
  "computer_use": {
    "enabled": false,
    "dry_run": true,
    "max_steps": 15,
    "timeout_secs": 300,
    "allow_third_party_fallback": false,
    "denylist": [
      "1password", "keepass", "bitwarden", "lastpass", "regedit",
      "cmd.exe", "powershell.exe", "mintty", "windowsterminal"
    ]
  },
  "remote": {
    "enabled": false,
    "paired_user_id": null,
    "allowed_roots": [],
    "allow_screenshots": false
  },
  "voice_live": {
    "model": "gemini-2.0-flash-exp",
    "max_session_duration_secs": 600
  },
  "tasks": {
    "allow_advanced_shell": false
  },
  "wake_word": {
    "enabled": false
  }
}
```

---

## 4. Manual Verification Checklist (Recommended Execution Order)

Execute manual tests in this strict order. Record outcomes in `docs/RESULTS_TEMPLATE.md`.

### Step 1: M0 Orb Window Presentation
- **Command**: `cargo run -p ruby-desktop`
- **Actions**:
  1. Inspect the desktop overlay.
  2. Verify the orb window renders on a transparent, borderless canvas with zero standard window drop-shadows or taskbar buttons.
  3. Verify the liquid orb visual animation is calm and has no blinking/pulsing badges.
  4. Toggle click-through in the tray/console and verify mouse clicks pass directly through to desktop windows underneath.

### Step 2: M1 Dictation Engine (`--diagnostic`)
- **Command**: `cargo run -p ruby-desktop -- --diagnostic`
- **Actions**:
  1. **Hold-to-Talk**: Focus a text editor (e.g. Notepad). Press and hold `Ctrl+Win` for >= 250ms. Verify orb shifts to Listening (Gold/Amber). Speak a short phrase: *"The quick brown fox jumps over the lazy dog."* Release keys.
  2. Verify orb enters Thinking, then returns to Idle. Text appears in Notepad via simulated Unicode SendInput.
  3. **Esc Cancel**: Hold `Ctrl+Win`, begin speaking, press `Esc`. Release `Ctrl+Win`. Verify dictation buffer is cancelled with zero text inserted.
  4. **Double-Tap Hands-Free**: Rapidly double-tap `Ctrl+Win` (< 350ms). Verify recording latches on hands-free. Single tap `Ctrl+Win` to commit and insert.

### Step 3: R0 Remote Pairing & Telegram Connection
- **Prerequisites**: Set `TELEGRAM_BOT_TOKEN`. Set `"enabled": true` in `remote_config.json`.
- **Actions**:
  1. Launch Ruby: `cargo run -p ruby-desktop`.
  2. From your personal Telegram client, send `/start` to the bot.
  3. Bot responds with an unauthenticated pairing prompt.
  4. Open Console View in Ruby. Note the 4-digit CSPRNG pairing code.
  5. In Telegram, send `/pair <code>`.
  6. Verify bot replies with successful pairing confirmation. Verify 5 wrong attempts invalidate the code.

### Step 4: R1 Allowed Roots & Tool Sandboxing
- **Actions**:
  1. In `%LOCALAPPDATA%\Ruby\remote_config.json`, configure `allowed_roots: ["D:\\projects\\ruby-agent\\workspace"]`.
  2. Over Telegram, message: `/status`. Verify battery, uptime, and system status return cleanly.
  3. Request file read inside `workspace/` -> succeeds.
  4. Request file read outside roots (e.g. `/read C:\Windows\win.ini`) -> refused with `OutsideAllowedRoots`.

### Step 5: M2 Console Chat & Affective Emotion System
- **Actions**:
  1. Open Console window (`http://localhost:1420` or desktop console).
  2. Send query: *"Can you explain the difference between a mutex and a semaphore?"*
  3. Verify response streams into console.
  4. Share a distress message: *"I had a really difficult and exhausting day today."*
  5. Verify orb switches smoothly to Empathetic mood (Warm Rose/Sage) without black core artifacts.

### Step 6: M3 MCP Registry & Memory Confirmation
- **Actions**:
  1. In Console, enter memory: `/add_memory fact "User preferred programming language is Rust"`.
  2. Query memory: `/search_memories "Rust"`. Verify result returns formatted in passive `[MEMORY_DATA]` tags.
  3. Test Delete: Trigger `/delete_memory <id>`. Verify console returns: *"Requires confirmation... Reply with code <code>"*.
  4. Send confirmation code. Verify memory is deleted.
  5. Test Wipe All: Trigger `/wipe_all_memories`. Verify prompt says: *"Wipe all memories (total N records)"* with exact count.

### Step 7: V1 Voice Live Streaming
- **Actions**:
  1. In Console, click **Start Live**.
  2. Verify static, calm "Mic Open" status indicator appears (no pulsing/flashing animations).
  3. Speak a conversational question. Verify real-time bidirectional audio response plays back.
  4. Press **Esc** or click **Stop Live**. Verify session immediately halts and transitions to Idle.

### Step 8: M4 Live Computer Use (Kill Switch Tested First!)
- **Prerequisites**: In `config.json`, set `"computer_use": { "enabled": true, "dry_run": false }`.
- **CRITICAL SAFETY CHECK**: Open Notepad and place it in the foreground.
- **Actions**:
  1. **Kill Switch Test**: Launch a mock computer use task: `start_computer_use_task("Type hello into Notepad")`.
  2. **Immediately press Esc or the global emergency hotkey (`Ctrl+Alt+Shift+R`)**.
  3. Verify the agent loop aborts immediately, releases any held mouse buttons or keys, and reports `TaskStatus::Cancelled`.
  4. **Password Field Guard**: Focus a login dialog or KeePass password field. Command Ruby to type text. Verify action is refused with `TypeGuardrailRefusal("Password field detected")`.
  5. **Session Lock Guard**: Lock the workstation (`Win+L`). Verify computer use loop pauses/aborts immediately.

### Step 9: M5 Background Tasks & Supervisor
- **Actions**:
  1. From Console or Telegram, submit a task: `/job submit "echo Background task test"`.
  2. Check status: `/jobs` and `/job <id>`.
  3. Inspect logs: `/logs <id>`.
  4. Test cancellation: `/cancel <id>`. Verify Windows Job Object terminates the underlying process tree.

### Step 10: R2 Remote Screen Capture Safety
- **Actions**:
  1. Ensure `"allow_screenshots": false` (default). Send `/screen` over Telegram -> verify refusal.
  2. Set `"allow_screenshots": true`. Send `/screen` -> verify confirmation prompt: *"Confirm remote screen capture? WARNING: Telegram bot messages are not end-to-end encrypted. Reply with /yes <code>"*.
  3. Confirm with `/yes <code>` -> verify full desktop screenshot arrives.
  4. Open KeePass or a password manager window. Send `/screen` and confirm -> verify capture is refused because a denylisted window is visible.
