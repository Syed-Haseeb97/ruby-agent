# E2E Scenario E5: Session Lock Mode Split (Mode A Paused vs Mode B Continued)

**Execution Timestamp**: 1795103714
**Session Event**: `WTS_SESSION_LOCK` (Workstation locked)
**Mode A Task**: `task-4bf39f33` (GUI Autonomous)
**Mode B Task**: `97f3151d-64e7-4600-ad72-11b33de776da` (Background Non-GUI Job)

## Transcript Exchange
1. **[System Event]**: Windows Session locked by user (`Win+L`).
2. **[Mode A Safety Interceptor]**: Mode A requires interactive GUI desktop. Autonomous actions halted immediately.
3. **[Ruby -> Phone]**: `Desktop session locked: Mode A autonomous task paused for security. Mode B background tasks continue running.`
4. **[State Split Verification]**:
- Mode A Task (`task-4bf39f33`): Status = `needs_input` (PAUSED)
- Mode B Job (`97f3151d-64e7-4600-ad72-11b33de776da`): Status = `Completed` (CONTINUED EXECUTION)
5. **[Background Supervisor]**: Mode B Job `97f3151d-64e7-4600-ad72-11b33de776da` completed in background.
6. **[Ruby -> Phone]**: `Background Job 97f3151d-64e7-4600-ad72-11b33de776da completed successfully.`
