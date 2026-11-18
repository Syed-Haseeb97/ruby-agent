# Milestone M4 Acceptance Report: Live Computer Use (Mode A)

**Author**: Ruby Assistant Team  
**Date**: 2026-11-18  
**Repository**: `Syed-Haseeb97/ruby-agent`  
**Milestone**: M4 (Live Computer Use - Mode A)

---

## 1. Executive Summary

Milestone M4 delivers Ruby's **Mode A Live Computer Use** engine: a native Windows autonomous desktop operator powered by Gemini multimodal vision reasoning and Win32 `SendInput`. Built with strict fail-closed safety policies in `ruby-policy`, the agent features an immediate-abort global kill switch, window/process security denylisting, secret typing prevention, prompt injection containment, consequential action confirmation gating, and window capture occlusion (`WDA_EXCLUDEFROMCAPTURE`) ensuring Ruby's Orb and Console never appear in screenshots.

---

## 2. Architecture & Deliverables

### A. Coordinate Mapping & Action Space (`ruby-actions::computer_use::action`)
- **Virtual Coordinate Normalization**: Maps between normalized $[0, 1000]$ space (Gemini Computer Use standard) and monitor physical pixels via per-monitor DPI scaling.
- **Action Primitive Set**: `Click`, `DoubleClick`, `TripleClick`, `RightClick`, `Move`, `Drag`, `Scroll`, `TypeText`, `PressKey`, `KeyDown`, `KeyUp`, `Hotkey`, `Wait`, `Finish`, `Fail`.
- **Unit Tested**: Verified coordinate boundary clamping and serde roundtrip via `test_coordinate_mapping` and `test_action_serialization_and_deserialization`.

### B. Input & Capture Execution (`ruby-actions::computer_use::executor`)
- **Native Windows SendInput**: Dispatches low-level mouse movements (`MOUSEEVENTF_ABSOLUTE`), left/right/middle button events, wheel scrolls, and Unicode keyboard events.
- **GDI Screen Capture**: Thread-safe in-memory BitBlt capture, automatic image resizing to $\le 1280$px width to reduce latency, and in-memory JPEG compression via `image` crate (zero disk writes).
- **Window Occlusion**: Win32 `SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)` applied to Orb and Console windows, eliminating recursive visual capture loops.
- **Hermetic Mock Executor**: `MockComputerUseExecutor` records actions and captures simulated frames for offline tests without touching physical hardware.

### C. Autonomous Agent Loop (`ruby-actions::computer_use::agent_loop`)
- **Loop Limits**: Configurable max step count (default 30) and wall-clock timeout (default 300s).
- **Stuck Detector**: Halts execution when identical actions repeat 3 times consecutively.
- **Global Kill Switch**: Async `KillSwitchToken` triggered by physical `Escape` key, Console button, or Telegram `/stop`. Immediately cancels in-flight model reasoning calls, releases held inputs (`release_all_held_inputs`), and halts the task.
- **Dry-Run Mode**: Allows observing and logging proposed actions in the Console without dispatching hardware input events.

### D. Security Boundaries & Invariants (`ruby-policy::computer_policy`)
- **Window Denylist**: Automatically halts when foreground window matches password managers (`1Password`, `Bitwarden`, `KeePass`, `LastPass`), banking portals (`Chase`, `PayPal`, `Wells Fargo`), or Windows Security / UAC dialogs.
- **Secret Typing Refusal**: Strict regex filter blocks API keys (`AIza...`, `sk-...`), GitHub tokens, and private keys before `TypeText` dispatch.
- **Prompt Injection Defense**: All screen content and OCR data are framed with `[SCREEN_DATA]` passive framing, instructing the model to treat visual text strictly as untrusted user data.
- **Consequential Action Gating**: High-risk actions (purchases, file deletions, settings changes) require a CSPRNG 4-digit confirmation code (`/yes <code>`) with 120-second TTL.

### E. Frontend & Mobile Channels
- **Console Panel**: Dedicated Computer Use panel with goal input, dry-run toggle, live step log, kill switch controls, and confirmation banner (strict editorial light-mode aesthetics: `#F5F3EE` canvas, `#C8C2B5` hairline borders, zero pulsing/blinking dots).
- **Acting Orb State**: Added `Acting` operational state (warm bronze amber `#86560D`, active tracking eye geometry).
- **Telegram Remote**: Added `/do <task>` command to initiate Mode A tasks from mobile, `/stop` to abort, and consequential confirmation prompts (`/yes <code>`). Screenshot sending remains disabled (reserved for Phase R2).

---

## 3. Verification Evidence Matrix

| Item | Description | Level | Evidence / Named Test |
|---|---|---|---|
| **B1.1** | Virtual-to-physical coordinate mapping ($0..1000 \to \text{px}$) | **VERIFIED** | `ruby-actions::computer_use::action::tests::test_coordinate_mapping` |
| **B1.2** | Action serialization & deserialization | **VERIFIED** | `ruby-actions::computer_use::action::tests::test_action_serialization_and_deserialization` |
| **B1.3** | SendInput mouse & keyboard dispatch | `compiles-only` | Native Win32 SendInput implemented in `WindowsComputerUseExecutor` (physical cursor movement unobserved) |
| **B1.4** | In-memory GDI capture + `WDA_EXCLUDEFROMCAPTURE` | `compiles-only` | Implemented in `ruby-desktop::setup` and `WindowsComputerUseExecutor` |
| **B2.1** | Agent loop success path with mock model & screen | **VERIFIED** | `ruby-actions::computer_use::agent_loop::tests::test_agent_loop_success_path` |
| **B2.2** | Kill switch abort while model reasoning hangs | **VERIFIED** | `ruby-actions::computer_use::agent_loop::tests::test_kill_switch_aborts_hanging_model_call` |
| **B2.3** | Loop limits: stuck action repetition detector (3x) | **VERIFIED** | `ruby-actions::computer_use::agent_loop::tests::test_stuck_detector_aborts_loop` |
| **B2.4** | Loop limits: max step limit exceeded | **VERIFIED** | `ruby-actions::computer_use::agent_loop::tests::test_max_steps_limit_exceeded` |
| **B2.5** | Loop limits: wall-clock timeout exceeded | **VERIFIED** | `ruby-actions::computer_use::agent_loop::tests::test_wall_clock_timeout_exceeded` |
| **B2.6** | Dry-run mode logging without input dispatch | **VERIFIED** | `ruby-actions::computer_use::agent_loop::tests::test_dry_run_mode_records_without_executing` |
| **B3.1** | Window denylist (password managers, banking, UAC) | **VERIFIED** | `ruby-policy::computer_policy::tests::test_denylist_catches_password_managers_and_banking` |
| **B3.2** | Secret typing refusal (Google keys, OpenAI, tokens) | **VERIFIED** | `ruby-policy::computer_policy::tests::test_secret_typing_refusal` & `test_secret_typing_refused` |
| **B3.3** | Screen data passive prompt injection containment | **VERIFIED** | `ruby-policy::computer_policy::tests::test_prompt_injection_passive_data_framing` |
| **B3.4** | Consequential action detection & gating | **VERIFIED** | `ruby-policy::computer_policy::tests::test_consequential_action_detection` |
| **B4.1** | Console Computer Use panel & live step log | `compiles-only` | Implemented in `ConsoleView.tsx`, verified via `npm run build` (vite v6.4.6) |
| **B4.2** | Acting Orb state visual rendering | `UNVERIFIED` | Added to `LiquidOrbCanvas.tsx`; marked UNVERIFIED for SSIM (PyQt commit `7bc2c3a` had no Acting state) |
| **B5.1** | Telegram `/do <task>` command | **VERIFIED** | `ruby-remote::tests::mock_telegram_tests::test_telegram_do_command_and_consequential_confirmation` |
| **B5.2** | Telegram `/stop` cancel all active tasks | **VERIFIED** | `ruby-remote::tests::mock_telegram_tests::test_stop_cancels_all_active_tasks` |
| **B5.3** | Telegram `/yes <code>` confirmation plumbing | **VERIFIED** | `ruby-remote::tests::mock_telegram_tests::test_confirmation_plumbing` |
| **B6.1** | Manual testing protocol documentation | **VERIFIED** | Documented in `docs/M4_MANUAL_TESTING.md` |
