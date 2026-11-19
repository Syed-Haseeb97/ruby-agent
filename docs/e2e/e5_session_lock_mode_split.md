# E2E Scenario E5: Session Lock Mode Split (Mode A Paused vs Mode B Continued)

**Execution Timestamp**: 1795105256
**Session Event**: `WTS_SESSION_LOCK` (Workstation locked)
**Mode A Task**: `task-e70ec47a` (GUI Autonomous)
**Mode B Task**: `a10b5ced-9d20-4626-a30e-1af22bac49e5` (Background Non-GUI Job)

## Transcript Exchange
1. **[System Event]**: Windows Session locked by user (`Win+L`).
2. **[Mode A Safety Interceptor]**: Mode A requires interactive GUI desktop. Autonomous actions halted immediately.
3. **[Ruby -> Phone]**: `Desktop session locked: Mode A autonomous task paused for security. Mode B background tasks continue running.`
4. **[State Split Verification]**:
- Mode A Task (`task-e70ec47a`): Status = `needs_input` (PAUSED)
- Mode B Job (`a10b5ced-9d20-4626-a30e-1af22bac49e5`): Status = `Completed` (CONTINUED EXECUTION)
5. **[Background Supervisor]**: Mode B Job `a10b5ced-9d20-4626-a30e-1af22bac49e5` completed in background.
6. **[Ruby -> Phone]**: `Background Job a10b5ced-9d20-4626-a30e-1af22bac49e5 completed successfully.`
