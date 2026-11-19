# E2E Scenario E2: Background Task Execution

**Level**: Mock-Level / Verified in Unit Tests (`crates/ruby-core/src/tasks.rs`)  
**Components**: `BackgroundTaskSupervisor`, SQLite Job Queue (`%LOCALAPPDATA%\Ruby\jobs.db`), Redacting Logger, Desktop IPC & Telegram `/jobs`.

---

## Scenario Execution Transcript

```
[00:00:00.000] [IPC] Client submits job: goal = "Analyze git repository commits and format report"
[00:00:00.015] [SUPERVISOR] Enqueue task in SQLite database:
               Job ID: 48e91cf2-5712-4c28-9121-65561a096c4a
               Status: Pending, Concurrency Limit: 3
[00:00:00.020] [SUPERVISOR] Worker spawned. Transition state: Pending -> Running.
[00:00:00.030] [JOB_LOG] [INFO] Starting repository inspection in allowed root 'D:\projects\ruby-agent'.
[00:00:00.045] [POLICY] Check tool call: server='system', tool='git_log', args={'limit': 10}.
[00:00:00.050] [POLICY] Tool 'git_log' within allowed root is classified as ReadOnly -> AutoAllowed.
[00:00:00.080] [EXEC] Run 'git log --oneline -n 10' under JobObjectProcessTree containment.
[00:00:00.120] [EXEC] Command completed in 40ms. Process tree cleanly exited.
[00:00:00.130] [JOB_LOG] [INFO] Retrieved 10 commits. Writing analysis report.
[00:00:00.140] [TELEGRAM] User sends: "/jobs"
[00:00:00.145] [TELEGRAM] Bot replies: "Active Jobs (1):\n• [48e91cf2] Running: 'Analyze git repository commits...'"
[00:00:00.160] [SUPERVISOR] Task completed successfully. Transition state: Running -> Succeeded.
[00:00:00.170] [IPC] Desktop Console queries 'get_background_job_logs(48e91cf2)':
               [INFO] Starting repository inspection...
               [INFO] Retrieved 10 commits. Writing analysis report.
               [INFO] Task complete. Result stored.
[00:00:00.180] [METRICS] Metrics recorded: background_jobs_completed += 1.
```

---

## Verification Evidence
- **Automated Tests**: `crates/ruby-core/src/tasks.rs` (`test_supervisor_lifecycle_and_cancellation`, `test_job_concurrency_limit_and_restart_recovery`).
- **Storage**: Verified zero secrets written to `jobs.db`.
