# E2E Scenario E6: Voice Live Session with Policy Gating

**Level**: Mock-Level / Verified in Unit Tests (`crates/ruby-voice/src/voice_live.rs`)  
**Components**: Voice Live Session Manager, WebSocket Audio Streamer, `ruby-policy` Confirmation Manager, Ephemeral 6-digit Codes.

---

## Scenario Execution Transcript

```
[00:00:00.000] [CONSOLE] User clicks "Start Voice Live" button in Ruby Console.
[00:00:00.010] [SESSION] State transition: Idle -> Connecting.
[00:00:00.020] [WEBSOCKET] Connect to Gemini Live Endpoint (wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContent).
[00:00:00.150] [WEBSOCKET] Handshake established. State transition: Connecting -> Active.
[00:00:00.160] [AUDIO] Microphone opened (16 kHz 16-bit PCM streaming).
[00:00:00.170] [CONSOLE] UI displays static "Mic Open" badge (0 blinking/pulsing animations).
[00:00:02.500] [USER] User speaks: "Check the latest commits on the repository and then delete the old scratch folder."
[00:00:03.200] [MODEL] Gemini Live streams audio response + tool call request:
               Tool Call 1: `system::git_log` with `{'limit': 5}`
[00:00:03.210] [POLICY] Check `system::git_log` -> ReadOnly -> AutoAllowed.
[00:00:03.230] [EXEC] Retrieve commits. Return DATA block to model:
               [TOOL_RESULT: system::git_log]
               e98a12c Fix path canonicalization
               71f3dba Add structured logging
               [/TOOL_RESULT]
[00:00:04.100] [MODEL] Tool Call 2: `system::delete_folder` with `{'path': 'D:\\projects\\ruby-agent\\scratch'}`
[00:00:04.110] [POLICY] Check `system::delete_folder` -> Destructive -> RequiresConfirmation!
[00:00:04.115] [CONFIRMATION] Register pending action in UnifiedConfirmationManager:
               Action: "Delete folder D:\projects\ruby-agent\scratch"
               Generated 6-digit CSPRNG code: "839201" (TTL: 120s)
[00:00:04.120] [CONSOLE] Console modal displays confirmation prompt with code: "839201".
[00:00:04.125] [SECURITY] Note: The code is NEVER passed back to the model output stream, preventing prompt injection bypass.
[00:00:06.000] [USER] User clicks "Confirm" in Console entering code "839201".
[00:00:06.010] [CONFIRMATION] Code validated and consumed!
[00:00:06.020] [EXEC] Directory deleted. Return DATA block:
               [TOOL_RESULT: system::delete_folder]
               Directory removed successfully.
               [/TOOL_RESULT]
[00:00:07.500] [USER] User presses Escape key once (single tap).
[00:00:07.505] [ESC_HIERARCHY] Esc tap dispatches: ComputerUse=Idle -> Voice Live ACTIVE -> Voice Live STOPPED.
[00:00:07.510] [SESSION] State transition: Active -> Idle. Microphone closed. WebSocket disconnected.
```

---

## Verification Evidence
- **Automated Tests**:
  - `crates/ruby-voice/src/voice_live.rs` (`test_voice_live_tool_call_policy_gating_and_data_return`).
  - Esc hierarchy wiring: `crates/ruby-desktop/src/commands.rs` (`test_esc_hierarchy_dispatch_mode_a_then_voice_then_dictation`).
  - Single instance & status transitions: `test_voice_live_double_start_rejected`.
