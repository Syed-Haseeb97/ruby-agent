# Phase R0 Manual Verification Guide: Telegram Channel

This guide describes manual end-to-end verification of Ruby's Telegram Remote Control channel (Phase R0).

---

## 1. Prerequisites & BotFather Setup

1. Open Telegram on your phone or desktop and message `@BotFather`.
2. Send `/newbot` and follow the prompts to name your bot (e.g., `MyRubyAssistantBot`) and choose a username ending in `bot`.
3. `@BotFather` will generate an HTTP API token formatted as `<bot_id>:<token>` (e.g., `123456789:AA...`).
   > **SECURITY WARNING**: Never paste, commit, or log your Telegram bot token.

4. Store the token in Windows Credential Manager or set the environment variable:
   ```cmd
   set TELEGRAM_BOT_TOKEN=your_token_here
   ```
   Or place it in `D:\projects\ruby-agent\.env` (which is strictly excluded by `.gitignore`).

5. Ensure remote control is enabled in `~/.ruby/remote_config.json`:
   ```json
   {
     "enabled": true,
     "keep_awake": true,
     "paired_user_id": null
   }
   ```

---

## 2. Pairing Test Flow

1. Launch Ruby Desktop:
   ```cmd
   cargo run -p ruby-desktop
   ```
2. In the Editorial Console UI, locate the **Telegram Remote Control** card.
3. Observe the **Active Pairing Code**:
   - A single-use 6-digit numeric code (e.g. `482910`) is displayed with a 5-minute (300s) countdown timer.
   - Notice that this code is shown *only* in the Console UI and is *never* written to logs or sent over the network.
4. From your Telegram app, message your bot:
   ```text
   /pair 482910
   ```
5. **Expected Result**:
   - Bot immediately replies:
     ```text
     ✅ Paired successfully with Ruby Assistant.
     Use /help to view available commands.
     ```
   - In the local Console UI, the badge immediately updates from `UNPAIRED` to:
     `PAIRED • USER <your_telegram_id>`
   - The pairing code disappears from the Console.

---

## 3. Second Account / Unauthorized Sender Test

1. From a second Telegram account or a friend's phone, search for your bot and send:
   ```text
   Hello Ruby
   ```
2. **Expected Result**:
   - The bot remains **completely silent** (no error message, no reply, zero network response) to prevent revealing the bot's existence or status.
   - In Ruby's local debug log (`~/.ruby/remote_audit.log`), verify:
     ```json
     {"timestamp":"...","sender_id":999999,"task_id":null,"command":"unauthorized","outcome":"dropped_silently"}
     ```
   - Verify that the message body (`"Hello Ruby"`) is **never** written to any log.

---

## 4. Offline / Staleness Test (Laptop Sleep / Disconnect)

1. Disconnect Wi-Fi on the laptop or put the laptop to sleep.
2. From your paired Telegram account, send a command:
   ```text
   Check status of downloads
   ```
3. Wait at least **3 minutes** (180 seconds, exceeding the 120-second staleness threshold).
4. Reconnect Wi-Fi or wake the laptop.
5. **Expected Result**:
   - Ruby retrieves the queued update during its long poll, compares the message timestamp with system clock ($> 120\text{ s}$ elapsed), and drops the message.
   - Bot replies:
     ```text
     Laptop was offline — resend if still needed
     ```
   - No task is scheduled or executed.

---

## 5. Built-in Commands Verification

From your paired phone, test the core Phase R0 commands:

1. **/status**:
   - Send: `/status`
   - Expected Reply:
     ```text
     Ruby Host Status:
     • Power: AC Power (or Battery %)
     • System Uptime: Xh Ym
     • Active Tasks: 0
     ```

2. **/tasks**:
   - Send: `/tasks`
   - Expected Reply:
     ```text
     No active tasks.
     ```

3. **Plain text task stub**:
   - Send: `Summarize my meetings`
   - Expected Reply:
     ```text
     Task [ID] created: received, tool execution isn't enabled yet
     ```

4. **/stop**:
   - Send: `/stop`
   - Expected Reply:
     ```text
     Cancelled X active tasks.
     ```

5. **/help**:
   - Send: `/help`
   - Expected Reply: Lists `/status`, `/tasks`, `/stop`, `/yes`, `/no`, `/help`.

---

## 6. Confirmation Plumbing Test (`/yes` and `/no`)

1. Trigger an action requiring confirmation (e.g. simulated dangerous action):
   - Bot prompts:
     ```text
     ⚠️ Action requires confirmation: Action Description
     Reply /yes 4819 to confirm or /no to reject (valid for 120 seconds).
     ```
2. Test wrong code:
   - Send: `/yes 0000`
   - Expected Reply: `❌ Incorrect confirmation code`
3. Test valid code:
   - Send: `/yes 4819`
   - Expected Reply: `✅ Confirmed action: Action Description`
4. Test rejection:
   - Send: `/no`
   - Expected Reply: `❌ Rejected action: Action Description`

---

## 7. Disconnect Remote Button Test

1. In the Ruby Console UI, click the **Disconnect remote** button.
2. **Expected Result**:
   - Console immediately updates to `UNPAIRED`.
   - Long polling halts immediately.
   - `~/.ruby/remote_config.json` has `"paired_user_id": null`.
   - Any subsequent message sent from the previously paired phone receives no response.
