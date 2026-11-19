# E2E Scenario E2: Phone Command Requiring Confirmation -> Background Job Lifecycle

**Execution Timestamp**: 1795105256
**User Identity**: Paired Telegram Client (ID: 888777)
**Requested Action**: `Run database migration and cluster index rebuild`
**Generated Confirmation Code**: `729621`

## Transcript Exchange
1. **[Phone -> Ruby]**: `/shell cargo run --bin migrate`
2. **[Ruby -> Phone]**: `Action requires confirmation: Run database migration and cluster index rebuild. Reply /yes <code> to execute.`
3. **[Phone -> Ruby]**: `/yes 000000`
4. **[Ruby -> Phone]**: `Invalid or expired confirmation code. Action not executed.`
5. **[Phone -> Ruby]**: `/yes 729621`
6. **[Ruby -> Phone]**: `Confirmation accepted. Submitting background job 7ad0472b-f469-4a64-a62e-032e1de26ce2...`
7. **[Background Supervisor]**: Job 7ad0472b-f469-4a64-a62e-032e1de26ce2 transitioned: Queued -> Running -> Completed.
8. **[Ruby -> Phone]**: `Job 7ad0472b-f469-4a64-a62e-032e1de26ce2 completed successfully in 120ms. Output: [MOCK_STDOUT_CLEAN]`
