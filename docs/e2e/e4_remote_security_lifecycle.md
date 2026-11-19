# E2E Scenario E4: Remote Security Lifecycle & Attack Defense

**Execution Timestamp**: 1795103713
**Defense 1**: Unknown Sender Silent Drop (Sender: `999999`)
**Defense 2**: Stale Message Expiry (Age: `150s`, Threshold: `60s`)
**Defense 3**: Replay Attack Blocked via Persisted Offset (`offset: 102`)
**Defense 4**: Mid-Task `/stop` Immediate Cancellation

## Transcript Exchange
1. **[Attacker -> Ruby]**: `/status` (Sender ID: 999999)
2. **[Ruby Security Filter]**: Sender is not paired. Silently dropped. Zero network responses emitted.
3. **[Owner -> Ruby]**: `/status` (Stale message, timestamp 150s old)
4. **[Ruby Security Filter]**: Message timestamp is older than 60s freshness threshold. Dropped silently.
5. **[Poller Restart]**: Poller restarted. Initial offset loaded from disk: `102`. Stale update `101` skipped.
6. **[Owner -> Ruby]**: `/stop` during active task `task-6479dd72`
7. **[Task Manager]**: Task `task-6479dd72` immediately transitioned: `Running -> Cancelled`. Child processes killed.
8. **[Ruby -> Owner]**: `All active tasks and live sessions have been stopped.`
