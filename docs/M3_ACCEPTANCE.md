# Milestone M3 Acceptance Report: Extensible Tools, Model Context Protocol (MCP) & Persistent Memory

**Repository**: `Syed-Haseeb97/ruby-agent`  
**Platform**: Windows 11 (Tauri 2.0 Native Shell)  
**Verification Date**: 2026-11-18  

---

## 1. Executive Summary

Milestone M3 equips Ruby with a safe, extensible tool execution engine, Model Context Protocol (MCP) client infrastructure, isolated headless browser sidecar, and an SQLite-backed FTS5 persistent memory system:
1. **Tool Security Policy (`ruby-policy`)**: Granular tool risk classification (`ReadOnly`, `Write`, `Destructive`, `ExternalNetwork`, `Unknown`). Fail-closed default deny on unclassified or unregistered tools. Strict SSRF protection preventing access to local loopback (127.0.0.1, localhost) and cloud metadata services (169.254.169.254) for network-bound tools.
2. **Explicit Confirmation Gate**: All `Write` and `Destructive` actions generate a cryptographic 4-digit confirmation token with 5-minute expiry, requiring user authorization via `/yes <code>` before execution.
3. **Robust Model Context Protocol (`ruby-mcp`)**: Stdio JSON-RPC 2.0 client with line-delimited message framing, 10s execution timeout, 64 KB output truncation, and process crash isolation (server terminations yield clean `McpError::Process` responses without crashing the host desktop agent).
4. **Isolated Playwright Sidecar**: Dedicated Microsoft Edge browser automation managed via pinned `@playwright/mcp@0.0.84`, using a sandboxed user profile (`D:\projects\ruby-agent\workspace\playwright_profile`) separate from user browser data. Lazy on-demand startup and automatic shutdown on `/stop` or idle timeout.
5. **Prompt Injection Containment**: All raw MCP tool outputs and external data are encapsulated inside explicit passive data boundary blocks (`[DATA_OUTPUT] ... [/DATA_OUTPUT]`), neutralizing indirect prompt injection attacks.
6. **SQLite FTS5 Persistent Memory (`ruby-memory`)**: Local full-text indexed memory store with fast BM25 ranked retrieval, schema migration, and strict secret/audio refusal refusing credentials (`AIza...`, `sk-...`, bot tokens), raw base64 data, and binary audio streams.
7. **Desktop & Chat Integration**: Deep integration into Tauri `AppState`, exposing IPC handlers for memory management, tool invocation, confirmation resolution, and chat shortcuts (`remember`, `recall`, `/yes <code>`).

---

## 2. Verification Matrix

| Spec Item | Description | Status | Verification Evidence |
|---|---|---|---|
| **B1.1** | Granular Risk Classification (`RiskClass`) | **VERIFIED** | Unit test `test_tool_security_policy_classification_and_default_deny` in `crates/ruby-policy/src/lib.rs` |
| **B1.2** | Fail-Closed Default Deny for Unknown Tools | **VERIFIED** | Unit test `test_unknown_tool_denied_by_default` in `crates/ruby-mcp/src/lib.rs` |
| **B1.3** | SSRF & Metadata IP Blocking | **VERIFIED** | Unit test `test_tool_security_policy_classification_and_default_deny` validating 127.0.0.1 and 169.254.169.254 denial |
| **B1.4** | Confirmation Token Generation & Expiry | **VERIFIED** | Unit test `test_write_and_destructive_tools_require_confirmation` in `crates/ruby-mcp/src/lib.rs` |
| **B2.1** | JSON-RPC 2.0 Stdio Framing & 10s Timeout | **mock-level** | Unit test `test_mcp_output_capped_at_64kb` & `StdioMcpClient` mock tests |
| **B2.2** | 64 KB MCP Tool Output Cap | **VERIFIED** | Unit test `test_mcp_output_capped_at_64kb` in `crates/ruby-mcp/src/lib.rs` |
| **B2.3** | Stdio MCP Process Crash Isolation | **VERIFIED** | Unit test `test_server_crash_isolation_returns_process_error_without_crashing` in `crates/ruby-mcp/src/lib.rs` |
| **B2.4** | Playwright Pinned Version & Isolated Profile | **VERIFIED** | Unit test `test_playwright_pinned_version_and_isolated_profile` in `crates/ruby-mcp/src/lib.rs` |
| **B2.5** | Prompt Injection Passive Delimiting | **VERIFIED** | Unit test `test_prompt_injection_in_mcp_output_treated_strictly_as_passive_data` in `crates/ruby-mcp/src/lib.rs` |
| **B2.6** | Server Registry Persistence (`mcp_servers.json`) | **VERIFIED** | Unit test `test_registry_load_and_save_mcp_servers` in `crates/ruby-mcp/src/lib.rs` |
| **B3.1** | SQLite FTS5 Full-Text Search & Ranked Recall | **VERIFIED** | Unit test `test_sqlite_memory_store_add_and_search` in `crates/ruby-memory/src/lib.rs` |
| **B3.2** | Memory Secret & Audio Payload Refusal | **VERIFIED** | Unit test `test_memory_refuses_secrets_and_audio_payloads` in `crates/ruby-memory/src/lib.rs` |
| **B3.3** | Memory Passive Formatting (`[FACT_DATABASE]`) | **VERIFIED** | Unit test `test_format_memories_as_passive_data` in `crates/ruby-memory/src/lib.rs` |
| **B3.4** | Windows Credential Manager & .env Fallback | **mock-level** | Unit test `test_credential_manager_memory_key_storage` in `crates/ruby-memory/src/lib.rs` |
| **B4.1** | Desktop IPC Commands (`execute_tool`, `add_memory`, etc.) | **compiles-only** | Registered in `generate_handler!` in `crates/ruby-desktop/src/lib.rs` |
| **B4.2** | Chat Shortcuts (`remember`, `recall`, `/yes <code>`) | **VERIFIED** | Unit test `test_chat_command_shortcuts_remember_recall_yes` in `crates/ruby-desktop/src/commands.rs` |
| **B4.3** | Live Playwright Browser Automation | **UNVERIFIED** | Requires active Node.js / npx `@playwright/mcp@0.0.84` execution on host system |
