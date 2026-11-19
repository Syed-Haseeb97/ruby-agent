# Part 2 — Settings, Onboarding & Doctor (U1–U5) Evidence

**Status**: COMPLETED
**Date**: 2026-11-19
**Scope**: Centralized Settings Panel (U1), First-Run Setup Wizard (U2), Diagnostic Doctor CLI & Modal (U3), Front-end Vitest Suite (U4), Evidence & Verification (U5).

---

## 1. U1: Centralized Settings Panel

- **Implementation**: `crates/ruby-desktop/frontend/src/SettingsPanel.tsx`, IPC commands in `crates/ruby-desktop/src/commands.rs`.
- **Aesthetic**:
  - Warm archival paper `#F5F3EE` canvas.
  - `#FFFFFF` cards with 1px stone hairline borders (`#C8C2B5`).
  - Font styling: `Instrument Serif` headers, `Geist Mono` status badges (`text-[11px]`, uppercase).
  - Status pills: academic green `#1D6B52` on sage `#E3EFE8`, warning amber `#86560D` on `#F6ECD9`, risk red `#B53A26` on `#F8E9E4`.
  - **Zero Blinking Dots**: All status pills and indicators are completely static; no `animate-pulse`, `animate-ping`, or blinking animations.
- **Validation**:
  - Validates `allowed_roots` on change. If an entire drive (e.g. `C:\`, `D:\`, `/`) is provided, displays prominent security advisory warning.
  - Enabling dangerous flags (`computer_use_enabled`, `advanced_shell_enabled`, `allow_screenshots`, `recording_enabled`) triggers an explicit confirmation modal requiring user acknowledgment before saving.
  - Safe defaults (`dry_run = true`, all dangerous flags `false`) save without modal.

---

## 2. U2: First-Run Setup Wizard

- **Implementation**: `crates/ruby-desktop/frontend/src/SetupWizard.tsx`.
- **4-Step Workflow**:
  1. **Welcome & Philosophy**: Explains non-autonomous defaults, local-first isolation in `%LOCALAPPDATA%\Ruby\`, and 1.2s hardware Esc kill switch.
  2. **Allowed Roots**: Defaults to `D:\projects\`, validates input, warns against whole-drive permissions.
  3. **Credentials Setup**: Queries `get_secrets_status` (names only, boolean flags, NEVER secret values).
  4. **Health Verification**: Runs `RubyDoctor::run_diagnostics()` pre-flight checklist and renders green pass checkmarks.
- **Triggers**: Invoked automatically on first launch if `config.json` does not exist (`is_first_run` IPC command).

---

## 3. U3: Diagnostic Doctor (`ruby doctor` CLI & Console Modal)

- **Implementation**: `crates/ruby-core/src/doctor.rs`, CLI entrypoint in `crates/ruby-desktop/src/main.rs`, IPC command in `commands.rs`, and UI modal in `DoctorModal.tsx`.
- **CLI Output Observed**:
```markdown
# Ruby Doctor Diagnostic Report (2026-11-19T13:56:08.924616700+00:00)
**Overall Status**: Warn

| Category | Check | Status | Details | Remedy |
| :--- | :--- | :---: | :--- | :--- |
| Credentials | Gemini API Key | PASS | Present in environment / Credential Manager | - |
| Credentials | ElevenLabs API Key | WARN | Not set (falls back to local Windows SAPI / Mock TTS) | Configure ELEVENLABS_API_KEY in Console Settings if cloud TTS is desired |
| Credentials | Telegram Bot Token | PASS | Present in environment / settings | - |
| Storage | Data Directory Isolation | PASS | Resolved workspace directory: C:\Users\ADMIN\AppData\Local\Ruby\workspace | - |
| Security | Computer Use Gating | PASS | enabled=false, dry_run=true | - |
| Security | Remote Screenshot Gating | PASS | allow_screenshots=false | - |
| Dependencies | git | PASS | Installed: git version 2.55.0.windows.2 (D:\Movies\Git\cmd\git.exe) [Version control operations & repo tools] | - |
| Dependencies | ffmpeg | WARN | Not found on PATH (Screen recording (optional)) | Install ffmpeg from https://ffmpeg.org/download.html and add to PATH |
| Dependencies | node | PASS | Installed: v22.23.1 (D:\Sigma\node js dudee\node.exe) [Playwright sidecar execution] | - |
| Display | Per-Monitor DPI Awareness | PASS | Per-Monitor V2 DPI awareness configured via application manifest | - |
| Storage | Disk Space | PASS | 13.5 GB free on data drive (minimum 5.0 GB recommended) | - |
| Network | API & Telegram Connectivity | PASS | Network test skipped (opt-in required) | - |
```

---

## 4. U4: Front-end Vitest Suite

- **File**: `crates/ruby-desktop/frontend/src/settings.test.tsx`
- **Results**: 5 passing tests (0 failures).
```
 ✓ src/settings.test.tsx (5 tests) 405ms
   ✓ SettingsPanel (U1) > renders dialog with proper accessibility role and no blinking/pulsing classes
   ✓ SettingsPanel (U1) > displays security advisory warning when root directory encompasses an entire drive
   ✓ SettingsPanel (U1) > triggers explicit confirmation modal when toggling dangerous security flags
   ✓ SetupWizard (U2) > guides through 4-step first-run wizard with accessibility and no blinking dots
   ✓ DoctorModal (U3) > renders diagnostic checklist items with static status badges and no pulses
```
- **Assertions**:
  - `container.innerHTML` tested with regex `/animate-pulse|animate-ping|blink/i` -> 0 matches found.
  - Dialog and alertdialog ARIA roles verified.
  - Production TypeScript & Vite build: 0 errors, 39 modules transformed.

---

## 5. Backend Unit Tests

- `crates/ruby-desktop/src/commands.rs`:
  - `test_save_app_config_requires_confirmation_for_dangerous_flags` (VERIFIED)
  - `test_save_app_config_warns_on_whole_drive_root` (VERIFIED)
  - `test_save_app_config_succeeds_with_safe_defaults` (VERIFIED)
  - `test_get_secrets_status_never_contains_values` (VERIFIED)
- `crates/ruby-core/src/doctor.rs`:
  - `test_doctor_diagnostics_report_generation` (VERIFIED)
