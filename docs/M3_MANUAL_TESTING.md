# Milestone M3: Manual Testing Guide

This guide provides step-by-step instructions for verifying Milestone M3 capabilities: Model Context Protocol (MCP) tool execution, policy gating, confirmation challenges, and persistent memory.

---

## 1. Tool Security Policy & Confirmation Verification

### 1.1 ReadOnly Tool (Automatic Execution)
1. Start the desktop application:
   ```powershell
   cargo run --bin ruby-desktop
   ```
2. In the chat interface or console, execute a read-only tool:
   - Tool name: `file_read`
   - Target: `D:\projects\ruby-agent\README.md`
3. **Expected Behavior**:
   - Security policy classifies `file_read` as `ReadOnly`.
   - Tool executes immediately without prompting for a confirmation code.
   - Result is returned wrapped in `[DATA_OUTPUT] ... [/DATA_OUTPUT]`.

### 1.2 Write / Destructive Tool (Confirmation Required)
1. In the chat interface, execute a modifying tool or command:
   - Command: `execute_tool` with `file_write`
2. **Expected Behavior**:
   - Security policy classifies `file_write` as `Write`.
   - System replies with a confirmation challenge:
     `"Confirmation required for action 'file_write'. Reply with: /yes <code> to proceed."`
   - Action is held in memory and expires after 300 seconds (5 minutes).
3. Authorize the action by typing:
   ```text
   /yes <code>
   ```
4. **Expected Behavior**:
   - The token matches the pending action.
   - Action executes and logs the outcome in the audit trail.
   - Any incorrect code or code entered after 5 minutes is rejected with an error.

### 1.3 Unknown Tool (Default Deny)
1. Attempt to execute an unapproved or arbitrary tool name (e.g. `remote_exec_shell`).
2. **Expected Behavior**:
   - Security policy classifies the tool as `Unknown`.
   - System immediately rejects the execution with `PermissionDenied: Tool is not registered in the security policy`.
   - No sub-process is spawned.

---

## 2. Persistent Memory (SQLite + FTS5) Verification

### 2.1 Storing Memories (`remember`)
1. In the chat interface, enter:
   ```text
   remember I prefer dark mode disabled and all badges strictly static.
   ```
2. **Expected Behavior**:
   - Ruby stores the entry into SQLite database at `D:\projects\ruby-agent\workspace\ruby_memory.db`.
   - The FTS5 index updates synchronously.
   - Ruby acknowledges: `"Memory stored: I prefer dark mode disabled..."`.

### 2.2 Recalling Memories (`recall`)
1. In the chat interface, enter:
   ```text
   recall badges
   ```
2. **Expected Behavior**:
   - FTS5 query executes across stored memory content.
   - Ruby returns matching memories formatted inside `[FACT_DATABASE] ... [/FACT_DATABASE]`.

### 2.3 Secret & Payload Refusal
1. In the chat interface, attempt to store a sensitive API key or audio chunk:
   ```text
   remember my key is AIzaSy-REDACTED-KEY-SAMPLE
   ```
2. **Expected Behavior**:
   - Ruby detects secret token pattern (`AIza...`).
   - Operation is rejected with: `"Memory rejected: Content contains secrets, tokens, or audio payloads."`
   - Database remains completely free of confidential credentials.

---

## 3. Playwright Browser Sidecar Verification

### 3.1 Pinned Version and Profile Isolation
1. Verify configured Playwright MCP parameters:
   - Command: `npx`
   - Args: `-y @playwright/mcp@0.0.84 --browser msedge --user-data-dir D:\projects\ruby-agent\workspace\playwright_profile`
2. **Expected Behavior**:
   - Browser launches under Microsoft Edge in an isolated profile directory.
   - Standard user browsing data, cookies, and saved passwords remain untouched.
3. Terminate sidecar:
   ```text
   /stop
   ```
4. **Expected Behavior**:
   - Sidecar process receives graceful shutdown signal.
