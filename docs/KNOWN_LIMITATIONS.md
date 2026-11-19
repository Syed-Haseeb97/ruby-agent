# Ruby Assistant — Known Limitations & Architectural Boundaries

## 1. Stubs & Disabled Features

| Subsystem | State | Rationale & Behavioral Boundary |
| :--- | :--- | :--- |
| **Wake Word Engine (`ruby-voice`)** | **Trait + Disabled Stub** | Default is `DisabledWakeWordDetector`. To eliminate acoustic surveillance risks, the microphone is never held open in the background without user presence. |
| **FreeLLMAPI Third-Party Fallback** | **Disabled by Default** | Default is `allow_fallback = false`. Third-party proxying of user transcripts, prompts, or screen captures is blocked unless explicitly enabled by the user in Settings. |
| **Computer Use (`ruby-actions`)** | **Disabled by Default (`dry_run = true`)** | Autonomous mouse and keyboard manipulation requires explicit opt-in in `config.toml` or Settings. When enabled, ships in `dry_run = true` mode until the user unchecks dry-run with explicit confirmation. |
| **Advanced Shell (`ruby-core`)** | **Disabled by Default** | Arbitrary shell execution is completely off. The background supervisor and action executor only allow allowlisted binaries (`git`, `notepad`, `calc`, `code`) under strict argument validation and Win32 Job Object containment. |
| **Remote Screenshots (`ruby-remote`)** | **Disabled by Default** | `/screen` via Telegram refuses by default. When enabled, requires explicit confirmation per capture, warning that Telegram bot messages are not end-to-end encrypted. |

---

## 2. Operations Requiring Human Confirmation

Under Ruby's safety policy, autonomous execution is strictly prohibited for the following operations; each requires an explicit CSPRNG 6-digit confirmation code via Desktop Console or paired Telegram:

1. **Memory Deletion & Wipe**:
   - `memory::delete`: Requires confirmation.
   - `memory::wipe_all`: Always requires confirmation showing the exact count of records to be deleted.
2. **File Deletion / Overwrite Outside Temp**:
   - Any write or delete action outside the configured allowed roots is blocked; actions within allowed roots that modify or destroy files require confirmation.
3. **Screen Recording Start**:
   - Starting a desktop video recording requires user authorization via Console button or Telegram `/rec` confirmation code.
4. **Dangerous Settings Toggles**:
   - Toggling `computer_use.enabled`, unchecking `dry_run`, or enabling `advanced_shell` in Settings requires explicit dialog confirmation.
5. **Screen Capture via Telegram**:
   - Every `/screen` capture request requires confirmation and checks all visible top-level windows against the security denylist.

---

## 3. Windows OS & Platform Limitations

1. **User Interface Privilege Isolation (UIPI)**:
   - When Ruby runs as a standard (non-elevated) process, Windows blocks `SendInput` and keyboard/mouse hooks from targeting elevated processes (e.g. Task Manager, elevated PowerShell, UAC prompts).
   - Ruby detects this condition and cleanly refuses input injection rather than hanging.
2. **Secure Desktop & Lock Screen**:
   - When the Windows workstation is locked (`Win+L`) or switched to the Secure Desktop (UAC credential dialog), global keyboard hooks and screen captures are blocked by the OS kernel.
   - Ruby's `SessionLockWatcher` automatically pauses active dictation and halts Computer Use agent loops on session lock.
3. **Per-Monitor V2 DPI Scaling**:
   - In heterogeneous multi-monitor setups with mixed DPI scalings (e.g. 100% and 150%), coordinate mapping requires Per-Monitor V2 awareness. Ruby enables this during initialization via `SetProcessDpiAwarenessContext`.
4. **DNS-Time SSRF vs DNS Rebinding**:
   - As documented in `ruby-policy`, pre-flight DNS checks (`to_socket_addrs`) cannot completely prevent DNS rebinding attacks where an authoritative nameserver switches from a public IP to `127.0.0.1` (TTL=0).
   - Ruby's primary defense against SSRF is strict domain allowlisting (`allowed_domains`) rather than point-in-time DNS checks.
