# Phase R2: Remote Screen Capture & Visual Verification — Manual Testing Guide

**Target Audience**: Haseeb (Live Hardware & Mobile Testing)  
**Date**: 2026-11-19  
**Repository**: `Syed-Haseeb97/ruby-agent`  

---

## 1. Prerequisites

1. Set your Telegram Bot Token in Windows Credential Manager or via environment variable:
   ```cmd
   set TELEGRAM_BOT_TOKEN=123456789:ABCdefGhIJKlmNoPQRsTUVwxyZ
   ```
2. Run Ruby Desktop in development mode:
   ```powershell
   cargo run -p ruby-desktop
   ```
3. Open the Ruby Desktop Control Console, copy the 6-digit pairing code (e.g. `839201`), and send `/pair 839201` from your mobile Telegram client to complete pairing.

---

## 2. Test Cases

### Test 1: Clean Desktop Screen Capture
1. Ensure a safe, non-sensitive window is in the foreground (e.g. VS Code or Notepad).
2. On your mobile Telegram chat, send:
   ```
   /screen
   ```
3. **Expected Result**:
   - The bot replies immediately with a native photo message.
   - Caption displays: `📸 Desktop Screenshot (1920x1080)` (or your monitor's native resolution).
   - In `%LOCALAPPDATA%\Ruby\remote_audit.log`, an entry appears:
     ```
     [...] sender=<your_id> task=- cmd=screen outcome=success
     ```

---

### Test 2: Security Denylist Refusal (Password Managers & Financials)
1. Open a password manager (1Password, Bitwarden, KeePass) or open a browser tab titled `Chase` or `PayPal: Summary`.
2. Ensure this sensitive window is focused as the foreground window.
3. On your mobile Telegram chat, send:
   ```
   /screen
   ```
4. **Expected Result**:
   - The bot refuses capture and replies:
     ```
     ❌ Screen capture refused: Foreground window "..." is protected by security denylist.
     ```
   - **Zero photos or image documents are transmitted.**
   - In `%LOCALAPPDATA%\Ruby\remote_audit.log`, an entry appears:
     ```
     [...] sender=<your_id> task=- cmd=screen outcome=refused_security_denylist
     ```

---

### Test 3: Windows Session Lock Protection
1. Press `Win + L` to lock your Windows workstation.
2. From your mobile Telegram chat, send:
   ```
   /screen
   ```
3. **Expected Result**:
   - The bot refuses capture and replies:
     ```
     ❌ Screen capture refused: Windows session is locked.
     ```
   - **Zero photos or image documents are transmitted.**
   - In `%LOCALAPPDATA%\Ruby\remote_audit.log`, an entry appears:
     ```
     [...] sender=<your_id> task=- cmd=screen outcome=refused_windows_session_is_locked
     ```

---

### Test 4: Elevated Process / Secure Desktop (UAC) Protection
1. Open an elevated Administrator PowerShell or trigger a Windows UAC credential dialog.
2. From your mobile Telegram chat, send:
   ```
   /screen
   ```
3. **Expected Result**:
   - The bot refuses capture and replies:
     ```
     ❌ Screen capture refused: Secure desktop or elevated process active.
     ```
   - **Zero photos or image documents are transmitted.**
   - In `%LOCALAPPDATA%\Ruby\remote_audit.log`, an entry appears:
     ```
     [...] sender=<your_id> task=- cmd=screen outcome=refused_secure_desktop_or_elevated_process_active
     ```

---

### Test 5: Help & Command Discovery
1. From your mobile Telegram chat, send:
   ```
   /help
   ```
2. **Expected Result**:
   - Command list includes:
     ```
     • /screen - Capture and receive current desktop screenshot
     ```
