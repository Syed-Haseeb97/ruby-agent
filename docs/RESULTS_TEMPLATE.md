# Ruby Operator Manual Verification Results

**Tester**: ___________________________  
**Date & Time**: ___________________________  
**Commit SHA**: `cb1ec21e2222258116a87ce671214d3f89b99662` (or current local HEAD)  
**Host Environment**: Windows 10 / 11 (Version: ______________________)  

---

## Verification Matrix

| Step | Milestone & Test Name | Expected Behavior | Observed Result (PASS / FAIL / SKIP) | Notes & Log Evidence |
| :--- | :--- | :--- | :--- | :--- |
| **1** | **M0: Orb Presentation** | Transparent, borderless window; calm liquid animation (no pulsing/flashing dots); click-through works. | | |
| **2** | **M1: Hold-to-Talk Dictation** | `Ctrl+Win` hold >=250ms turns orb gold; speech inserts into Notepad; early release cancels. | | |
| **3** | **M1: Esc Cancellation** | Pressing `Esc` during dictation discards pre-roll/audio buffer with 0 characters typed. | | |
| **4** | **M1: Hands-Free Double-Tap** | Double-tap `Ctrl+Win` latches recording; single tap inserts text. | | |
| **5** | **R0: Telegram Pairing** | `/start` generates pairing prompt; CSPRNG code displays in Console; `/pair <code>` authenticates; 5 wrong codes invalidate. | | |
| **6** | **R1: Allowed Roots Enforcement** | Reads inside `workspace/` succeed; reads outside (e.g. `C:\Windows`) rejected with `OutsideAllowedRoots`. | | |
| **7** | **M2: Console Chat & Empathy** | Chat streams correctly; distress prompt shifts orb to Empathetic without black core artifacts. | | |
| **8** | **M3: MCP Confirmation Gating** | Destructive tools require confirmation code; passive `[MEMORY_DATA]` tagging intact. | | |
| **9** | **M3: Memory Delete & Wipe** | `/delete_memory` requires code; `/wipe_all_memories` requires code and displays exact item count. | | |
| **10** | **V1: Voice Live Streaming** | Bidirectional audio streams in real-time; static "Mic Open" indicator shows; Esc/Stop returns to Idle. | | |
| **11** | **M4: Kill Switch Immediate Halt** | Emergency `Esc` or `Ctrl+Alt+Shift+R` halts CU loop immediately; releases all held keys/mouse buttons. | | |
| **12** | **M4: Password Field Guard** | Refuses typing when focused element has UIA `IsPassword` attribute. | | |
| **13** | **M4: Session Lock & UAC Guard** | Computer Use loop halts when Windows workstation is locked or elevated prompt active. | | |
| **14** | **M5: Background Job Supervisor** | `/jobs`, `/job <id>`, `/logs <id>` work; `/cancel` kills entire child process tree. | | |
| **15** | **R2: Remote Screen Disabled Default**| `/screen` rejected when `allow_screenshots: false` in config. | | |
| **16** | **R2: Remote Screen E2EE Notice** | When enabled, `/screen` requires confirmation quoting non-E2EE warning before dispatch. | | |
| **17** | **R2: Full Desktop Denylist Guard**| Refuses capture if KeePass / 1Password or other denylisted window is visible anywhere on screen. | | |

---

## Detailed Notes & Anomalies

```text
[Paste any terminal outputs, stack traces, or observed anomalies here]
```

---

## Final Certification
- [ ] All mandatory safety gates (kill switches, confirmation gating, root confinement, denylists) **PASSED**.
- [ ] Signed off for local development use.
