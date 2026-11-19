# Esc Key Hierarchy and Kill Switch Specification

This document defines the strict dispatch hierarchy and behavior for single-tap Escape vs. Escape-hold across all Ruby operational states, adhering to Checkpoint 5 specification Z1.

## Operational States

1. **Mode A (Computer Use Active)**: Autonomous or guided GUI interaction running via Gemini vision loop.
2. **Mode V (Voice Live Active)**: Real-time bidirectional streaming session with voice input/output.
3. **Mode D (Dictation Active)**: Push-to-talk audio capture and speech-to-text pipeline active.
4. **Mode B (Background Tasks Active)**: Asynchronous non-GUI jobs executing in the background supervisor.
5. **Idle State**: No active tasks, streams, recordings, or capture sessions.

---

## State × Action Dispatch Matrix

| System State | Esc Tap (< 1200 ms) Action | Esc Hold (≥ 1200 ms) Kill Switch Action | Process State | User Notification |
| :--- | :--- | :--- | :--- | :--- |
| **Mode A (Computer Use)** | Aborts active Computer Use loop; releases CU locks | Aborts CU, stops Voice Live, cancels Dictation, cancels background jobs, stops recording & Playwright, releases all synthetic mouse/keyboard inputs | **Stays RUNNING** | Console: static notice<br>Telegram: alert notice |
| **Mode V (Voice Live)** | Halts Voice Live stream; closes WebSocket; Orb returns to Idle | Halts Voice Live, cancels all active/queued operations, releases all synthetic mouse/keyboard inputs | **Stays RUNNING** | Console: static notice<br>Telegram: alert notice |
| **Mode D (Dictation)** | Cancels audio capture; discards speech buffer; resets hook | Cancels dictation, stops all background & voice operations, releases all synthetic mouse/keyboard inputs | **Stays RUNNING** | Console: static notice<br>Telegram: alert notice |
| **Mode B (Background Jobs)** | No-op (background jobs are detached and require explicit `/cancel <id>` or `/stop`) | Cancels all active and queued background tasks via supervisor; terminates child process trees | **Stays RUNNING** | Console: static notice<br>Telegram: alert notice |
| **Idle State** | No-op (dismisses frontmost modal dialog if open) | Safety sweep: forces release of any stuck synthetic inputs; asserts all subsystems are idle | **Stays RUNNING** | Console: static notice<br>Telegram: alert notice |

---

## Separation of Kill Switch vs. Application Quit

- **Esc Hold (Kill Switch)**:
  - Intended for emergency abort of autonomous actions or runaway tasks.
  - Releases mouse buttons (`VK_LBUTTON`, `VK_RBUTTON`, `VK_MBUTTON`) and modifier keys (`VK_CONTROL`, `VK_SHIFT`, `VK_MENU`).
  - Halts all child processes and background jobs.
  - **Does NOT exit the process (`process::exit` is strictly prohibited on this path).**
  - Leaves the Console and Telegram bot active so the user can inspect what happened or issue new commands.
- **Application Quit**:
  - Initiated strictly via explicit user intent:
    1. System Tray menu: **Quit**
    2. Paired Telegram bot: `/quit` command, which requires explicit confirmation code (`/yes <code>`).
  - Cleans up state, closes named mutexes, flushes logs, and performs graceful termination.
