# E2E Scenario E4: Telegram Pairing & Remote Command Isolation

**Level**: Mock-Level / Verified in Unit Tests (`crates/ruby-remote/tests/mock_telegram_tests.rs`)  
**Components**: Telegram Bot Adapter, R0 Pairing Challenge, R1 Policy-Gated Router, R2 Screenshot Security Guard.

---

## Scenario Execution Transcript

```
[00:00:00.000] [REMOTE] Ruby Remote Bot polling initialized.
[00:00:01.000] [USER] Sends Telegram message: "/start"
[00:00:01.010] [R0_PAIRING] Unpaired chat ID (99887766). Generate 6-digit challenge code: "492817" (TTL 120s).
[00:00:01.020] [TELEGRAM] Bot replies: "Ruby Pairing Request. Enter code 492817 in your local Ruby Desktop Console."
[00:00:04.500] [CONSOLE] User enters code "492817" into Console Settings -> Remote Pairing.
[00:00:04.510] [R0_PAIRING] Code verified! Chat ID 99887766 is now permanently paired.
[00:00:05.000] [USER] Sends: "/status"
[00:00:05.015] [TELEGRAM] Bot replies: "Ruby Assistant Status: Active\nPower: AC Online, Uptime: 4h 12m\nDictation: 15 runs, 0 errors."
[00:00:06.000] [USER] Sends: "/read D:\projects\ruby-agent\Cargo.toml"
[00:00:06.010] [POLICY] Check path 'D:\projects\ruby-agent\Cargo.toml' against allowed roots:
               Allowed roots: ['D:\projects\ruby-agent']
               Canonicalization check: PASSED.
[00:00:06.025] [TELEGRAM] Bot replies: "[Cargo.toml content truncated to 64 KB]..."
[00:00:07.500] [ATTACK] User or attacker sends: "/read C:\Windows\System32\drivers\etc\hosts"
[00:00:07.510] [POLICY] Canonical path 'C:\Windows\System32\drivers\etc\hosts' is OUTSIDE allowed roots!
[00:00:07.515] [SECURITY] PolicyDecision::Denied("Path outside allowed roots").
[00:00:07.520] [TELEGRAM] Bot replies: "â Œ Access Denied: Path outside allowed roots."
[00:00:08.500] [USER] Sends: "/screen"
[00:00:08.510] [SECURITY] Config check: `allow_screenshots` is FALSE by default.
[00:00:08.515] [TELEGRAM] Bot replies: "â Œ Screen capture is disabled in Ruby security configuration."
```

---

## Verification Evidence
- **Automated Tests**:
  - `crates/ruby-remote/tests/mock_telegram_tests.rs` (31/31 passed).
  - Pairing lifecycle: `test_pairing_success_and_replay_rejection`.
  - Allowed roots enforcement: `test_r1_read_path_inside_and_outside_allowed_roots`.
  - Screenshot disablement: `test_r2_screenshot_disabled_by_default`.
