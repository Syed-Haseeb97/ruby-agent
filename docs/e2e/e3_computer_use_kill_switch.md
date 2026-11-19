# E2E Scenario E3: Computer Use with Kill Switch

**Level**: Mock-Level / Verified in Unit Tests (`crates/ruby-actions/src/computer_use/`)  
**Components**: Computer Use Agent Loop, Mock/Real Win32 Input Synthesizer, 1200ms Esc Kill-Switch, Coordinate Normalizer.

---

## Scenario Execution Transcript

```
[00:00:00.000] [CONSOLE] User clicks "Start Computer Use Task": goal = "Organize files on desktop"
[00:00:00.020] [SECURITY] Consequential intent analysis: goal does not match irreversible operations.
[00:00:00.030] [SECURITY] Global safety check: Screen resolution 1920x1080, DPI 125%, DryRun = false.
[00:00:00.050] [AGENT_LOOP] Step 1: Capture screen (1920x1080 JPEG).
[00:00:00.070] [SECURITY] Denylist scan: 0 denylisted processes (no 1Password, Bitwarden, or Bank windows).
[00:00:00.080] [MODEL] Propose action: MouseDown at (450, 320) to begin drag selection.
[00:00:00.100] [SYNTHESIZER] Dispatch MouseDown (Left button held at normalized coords 450, 320).
[00:00:00.120] [AGENT_LOOP] Step 2: Dragging cursor towards (800, 600).
[00:00:00.400] [USER_EMERGENCY] User presses and holds the Escape key.
[00:00:01.600] [HOOK] Escape key held continuously for 1200 ms.
[00:00:01.605] [KILL_SWITCH] EMERGENCY KILL SWITCH TRIGGERED!
[00:00:01.610] [SYNTHESIZER] Emergency Input Release:
               - SendInput MouseUp (Left button released)
               - SendInput MouseUp (Right button released)
               - SendInput KeyUp for all virtual keys
[00:00:01.620] [AGENT_LOOP] Agent loop aborted immediately with `AbortReason::KillSwitchTriggered`.
[00:00:01.630] [STATE] Desktop Console updates status: Stopped ("Emergency kill switch activated. All inputs released.").
[00:00:01.640] [TELEGRAM] If paired, notification dispatched: "âš ï¸  Ruby Emergency Kill Switch triggered. Computer Use halted."
[00:00:01.650] [AUDIT] Audit event logged: action='kill_switch_triggered', zero typed text recorded.
```

---

## Verification Evidence
- **Automated Tests**:
  - `test_kill_switch_during_drag_releases_mouse` (`crates/ruby-actions/src/computer_use/agent_loop.rs`)
  - `test_kill_switch_during_held_key_releases_key` (`crates/ruby-actions/src/computer_use/agent_loop.rs`)
  - `test_kill_switch_aborts_hanging_model_call` (`crates/ruby-actions/src/computer_use/agent_loop.rs`)
