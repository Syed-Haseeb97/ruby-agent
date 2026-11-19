# Ruby Assistant — Key Rotation & Secrets Management Guide

## 1. Secrets Overview

Ruby Assistant manages several credentials and sensitive tokens across its operational subsystems:

| Secret | Purpose | Primary Storage | Fallback Storage | At-Rest Protection |
| :--- | :--- | :--- | :--- | :--- |
| **`GEMINI_API_KEY`** | Google Gemini API (Flash, Pro, Live, Vision) | Windows Credential Manager (`Ruby/GEMINI_API_KEY`) | Environment / `.env` | DPAPI-encrypted at rest |
| **`TELEGRAM_BOT_TOKEN`** | Telegram Bot API connection (R0â€“R2) | Windows Credential Manager (`Ruby/TELEGRAM_BOT_TOKEN`) | Environment / `.env` | DPAPI-encrypted at rest |
| **`FREELLMAPI_KEY`** | FreeLLMAPI third-party fallback (off by default) | Windows Credential Manager (`Ruby/FREELLMAPI_KEY`) | Environment / `.env` | DPAPI-encrypted at rest |
| **Telegram `chat_id`** | Paired Telegram control channel | `%LOCALAPPDATA%\Ruby\remote_config.json` | None | OS file ACL, 0 plaintext tokens |
| **Confirmation Codes** | Model & remote action authorization | In-memory ephemeral CSPRNG (120 s TTL) | Never persisted | Ephemeral RAM only |

### Zero Secrets At Rest In Databases and Logs
- **Memory Store (`ruby_memory.db`)**: Enforces `validate_memory_text()` on every insertion. Any text matching known secret prefixes (`ghp_`, `github_pat_`, `sk-`, `AIza`, Slack tokens, Telegram bot tokens, PEM headers, private keys) is rejected with `RefusedSensitiveContent`.
- **Background Tasks (`jobs.db`)**: Logs and goals pass through `RedactingWriter` / secret redaction masks before database insertion.
- **Audit & Application Logs (`%LOCALAPPDATA%\Ruby\logs\*.log`)**: All log lines pass through `RedactingWriter` implementing regex-based secret masking replacing sensitive tokens with `[REDACTED_SECRET]`.

---

## 2. Key Rotation Procedures

### A. Rotating the Gemini API Key
1. **Generate a new key** in Google AI Studio or Google Cloud Console.
2. **Update the secret in Ruby**:
   - **Method 1 (UI)**: Open Ruby Assistant Console -> **Settings** -> **Credentials** -> paste the new key into the `Gemini API Key` field -> click **Save**.
   - **Method 2 (CLI / Credential Manager)**:
     ```powershell
     cmdkey /generic:Ruby/GEMINI_API_KEY /user:Ruby /pass:"<NEW_GEMINI_KEY>"
     ```
   - **Method 3 (.env)**: Update `GEMINI_API_KEY=<NEW_GEMINI_KEY>` in `.env`.
3. **Verify**: Open Settings -> Run **Doctor** pre-flight check. Confirm `Gemini API Key: Available`.
4. **Revoke the old key** in Google AI Studio once verification passes.

### B. Rotating the Telegram Bot Token
1. **Revoke/Generate token** via `@BotFather` on Telegram (`/revoke` or `/token`).
2. **Update the token in Ruby**:
   - **Method 1 (UI)**: Open Ruby Assistant Console -> **Settings** -> **Credentials** -> paste the new token into `Telegram Bot Token` -> click **Save**.
   - **Method 2 (CLI)**:
     ```powershell
     cmdkey /generic:Ruby/TELEGRAM_BOT_TOKEN /user:Ruby /pass:"<NEW_BOT_TOKEN>"
     ```
3. **Restart the Remote Service**:
   - Disconnect in Settings, or restart Ruby Assistant.
4. **Re-pair User**:
   - In Telegram, send `/start` to receive a new pairing code.
   - Enter the pairing code in Ruby Console to re-bind the authorized chat ID.

### C. Rotating FreeLLMAPI Key (if used)
1. Obtain the new key from your FreeLLMAPI provider.
2. Update via Settings -> Credentials or `cmdkey /generic:Ruby/FREELLMAPI_KEY /user:Ruby /pass:"<NEW_KEY>"`.
3. Revoke the old key.

---

## 3. Data Sanitization and Wiping Local State

### A. Wiping Memory Database
To erase all stored facts, preferences, and entity associations:
- **Via Console**: Open Ruby Console -> **Settings** -> **Memories** -> Click **Wipe All Memories**. Confirm the action by entering the 6-digit confirmation code.
- **Via File System**:
  ```powershell
  Remove-Item "$env:LOCALAPPDATA\Ruby\ruby_memory.db*" -Force
  ```

### B. Wiping Task History and Audit Logs
```powershell
Remove-Item "$env:LOCALAPPDATA\Ruby\jobs.db*" -Force
Remove-Item "$env:LOCALAPPDATA\Ruby\logs\*.log" -Force
```

### C. Complete Application Reset (Factory Reset)
To completely remove all Ruby configuration, cached states, recordings, and databases:
```powershell
# 1. Terminate Ruby processes
Stop-Process -Name ruby-desktop -ErrorAction SilentlyContinue

# 2. Delete data directory
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\Ruby"

# 3. Clear Windows Credential Manager entries
cmdkey /delete:Ruby/GEMINI_API_KEY
cmdkey /delete:Ruby/TELEGRAM_BOT_TOKEN
cmdkey /delete:Ruby/FREELLMAPI_KEY
```
