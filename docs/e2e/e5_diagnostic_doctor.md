# E2E Scenario E5: Diagnostic Doctor Pre-Flight

**Level**: Mock-Level / Verified in Unit Tests (`crates/ruby-core/src/doctor.rs`)  
**Components**: Diagnostic Engine, Win32 System Checks, Security Flag Inspector, Credential Auditor.

---

## Scenario Execution Transcript

```
[00:00:00.000] [CLI] User executes: `target\debug\ruby-desktop.exe doctor --json`
[00:00:00.005] [DOCTOR] Initializing pre-flight diagnostic inspection...
[00:00:00.010] [CHECK 1: Storage Isolation]
               Inspecting database and log paths:
               - Memory DB: %LOCALAPPDATA%\Ruby\ruby_memory.db (Isolated on Drive D / LocalAppData)
               - Jobs DB:   %LOCALAPPDATA%\Ruby\jobs.db
               - Logs:      %LOCALAPPDATA%\Ruby\logs\
               Status: HEALTHY (Zero repo-polluting paths).
[00:00:00.015] [CHECK 2: Credentials Status]
               - GEMINI_API_KEY: Configured (Value Redacted: [AVAILABLE])
               - TELEGRAM_BOT_TOKEN: Configured (Value Redacted: [AVAILABLE])
               - FREELLMAPI_KEY: Not configured (Optional)
               Status: HEALTHY.
[00:00:00.020] [CHECK 3: Security Flags Defaults]
               - computer_use.enabled: false (Safe)
               - dry_run: true (Safe)
               - advanced_shell.enabled: false (Safe)
               - recording.enabled: false (Safe)
               - remote.allow_screenshots: false (Safe)
               Status: HEALTHY (All dangerous features ship OFF).
[00:00:00.025] [CHECK 4: External Tooling]
               - git: Installed (version 2.47.1)
               - ffmpeg: Detected in PATH
               - node: Installed (v22.12.0)
               Status: HEALTHY.
[00:00:00.030] [CHECK 5: Display & OS Properties]
               - DPI Awareness: Per-Monitor V2 enabled
               - Free Disk Space: 142.6 GB (> 5 GB minimum threshold)
               Status: HEALTHY.
[00:00:00.035] [DOCTOR] JSON report generated:
{
  "healthy": true,
  "timestamp": "2026-11-19T14:50:00Z",
  "checks": [
    { "name": "Storage Isolation", "status": "pass", "details": "Paths outside git repo" },
    { "name": "Credentials", "status": "pass", "details": "Keys present, 0 plaintext logged" },
    { "name": "Security Defaults", "status": "pass", "details": "All dangerous flags OFF" },
    { "name": "External Tools", "status": "pass", "details": "git, node, ffmpeg detected" },
    { "name": "System Resources", "status": "pass", "details": "142.6 GB free, DPI Per-Monitor V2" }
  ]
}
```

---

## Verification Evidence
- **Automated Tests**: `crates/ruby-core/src/doctor.rs` (`test_doctor_inspection_sanitized_and_healthy`).
- **CLI Subcommand**: Verified in `crates/ruby-desktop/src/main.rs`.
