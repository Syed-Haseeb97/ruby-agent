# E2E Scenario E2: Phone Command Requiring Confirmation -> Background Job Lifecycle

**Execution Timestamp**: 1795103714
**User Identity**: Paired Telegram Client (ID: 888777)
**Requested Action**: `Run database migration and cluster index rebuild`
**Generated Confirmation Code**: `658426`

## Transcript Exchange
1. **[Phone -> Ruby]**: `/shell cargo run --bin migrate`
2. **[Ruby -> Phone]**: `Action requires confirmation: Run database migration and cluster index rebuild. Reply /yes <code> to execute.`
3. **[Phone -> Ruby]**: `/yes 000000`
4. **[Ruby -> Phone]**: `Invalid or expired confirmation code. Action not executed.`
5. **[Phone -> Ruby]**: `/yes 658426`
6. **[Ruby -> Phone]**: `Confirmation accepted. Submitting background job 82718350-00fd-410a-a9e6-166bc3ecb61c...`
7. **[Background Supervisor]**: Job 82718350-00fd-410a-a9e6-166bc3ecb61c transitioned: Queued -> Running -> Completed.
8. **[Ruby -> Phone]**: `Job 82718350-00fd-410a-a9e6-166bc3ecb61c completed successfully in 120ms. Output: [MOCK_STDOUT_CLEAN]`
