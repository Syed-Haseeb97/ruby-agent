# Antigravity Brain Directory Inspection & Integration Notes (Phase R2)
**Document Path**: `docs/R2_ANTIGRAVITY_NOTES.md`  
**Evaluation Target**: `%USERPROFILE%\.gemini\antigravity\brain` and sibling structures  
**Status**: VERIFIED via direct filesystem inspection  

---

## 1. Physical Directory Structure & Evidence

Inspection of `%USERPROFILE%\.gemini\antigravity` reveals the following concrete layout:

```text
%USERPROFILE%\.gemini\antigravity/
├── brain/
│   ├── <conversation-uuid-1>/ (e.g. a25bc096-6802-4e59-be65-16c5e20cd525)
│   │   ├── .system_generated/
│   │   │   ├── logs/
│   │   │   │   ├── transcript.jsonl
│   │   │   │   └── transcript_full.jsonl
│   │   │   ├── steps/
│   │   │   │   └── <step-id>/output.txt
│   │   │   └── tasks/
│   │   │       └── <task-id>.log
│   │   ├── scratch/
│   │   ├── visuals/
│   │   └── *.md (Markdown Artifacts)
│   └── <conversation-uuid-n>/
├── builtin/
│   └── skills/
├── mcp/
│   └── <mcp-server-name>/
└── scratch/
```

### Sanitized Sample: `transcript.jsonl` Record
```json
{
  "step_index": 1,
  "source": "USER_EXPLICIT",
  "type": "USER_INPUT",
  "status": "DONE",
  "created_at": "2026-11-19T08:17:44Z",
  "content": "BATCH 3: fixes, then M4...",
  "tool_calls": []
}
```

---

## 2. Feasible vs. Fragile Categorization

### A. What is FEASIBLE (Reliably Derivable from Static Files)
1. **Conversation Identification**:
   - Each UUID directory under `brain/` corresponds 1:1 to an Antigravity conversation session.
2. **Timestamps & Recency**:
   - Filesystem metadata (`mtime` of `transcript.jsonl` or directory modification time) yields accurate last-activity sorting.
3. **Conversation Summaries & User Intent**:
   - Reading the initial entries in `transcript.jsonl` or parsing `<CONTEXT_SUMMARY>` headers provides reliable user goals and topics.
4. **Artifact Enumeration**:
   - Files directly in the root of each conversation folder (e.g. `*.md`, `*.png`) can be enumerated with accurate creation dates, file sizes, and headings.

### B. What is FRAGILE or IMPOSSIBLE (Not Derivable from Files Alone)
1. **Active "Running" State**:
   - **Finding**: Antigravity writes JSON lines synchronously or as steps complete, but does NOT maintain an active PID file, lockfile, or heartbeat on disk.
   - **Risk**: A conversation where the agent is actively thinking/generating appears structurally identical on disk to an idle or finished conversation. Inferring "running" from file recency is prone to false positives/negatives.
2. **"Unread" Status**:
   - **Finding**: There is no read/unread cursor or acknowledgment flag stored in `transcript.jsonl` or metadata.
   - **Risk**: Read/unread is an in-memory UI state of the Antigravity desktop application.
3. **Subagent Process Hierarchy**:
   - **Finding**: Although `invoke_subagent` calls are recorded in transcript logs, their live runtime state (whether child threads/processes are still alive or cancelled) exists only in the IDE host memory.

---

## 3. Implications for the Ruby `/ag` Command

The `/ag` command in Ruby (`ruby-remote`) is intentionally designed to fail-closed and avoid speculation:
- **Derives & Displays**:
  - List of recent conversation UUIDs.
  - Formatted last-modified timestamp (`YYYY-MM-DD HH:MM`).
  - First user prompt or extracted summary topic.
  - Count and filenames of generated artifacts.
- **Explicitly Disclaims**:
  - Does NOT display a synthetic "Active / Running" pill.
  - Does NOT guess "Unread" badges.
  - Emits clear notice: `"Real-time execution status and unread flags are internal to the Antigravity IDE and cannot be derived from static files."`
