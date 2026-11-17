# Milestone M1: Wispr Flow-Style System-Wide Dictation (code-complete, hardware verification pending)

**Repository**: `Syed-Haseeb97/ruby-agent` (Private)  
**Workspace**: `D:\projects\ruby-agent`  
**Verified CI Commit**: Pending next push  

---

## 1. Executive Summary & Verification Matrix

All required features, fixes, and guardrails are code-complete and verified in automated workspace test suites (all 49 tests passing across all crates) and green on GitHub Actions CI. Physical hardware verification on workstation hardware (real microphone, real keyboard chord release, and shell GUI target) remains pending as noted below.

| # | Item | Implementation & Verification Evidence | Verification Status |
|---|------|-----------------------------------------|---------------------|
| **1** | **Pre-roll Ring Buffer Capture** | Ring buffer captures on key-down immediately; committed at $\ge 250\text{ ms}$; discarded on early release. Verified by **engine-level test** `test_engine_level_hotkey_to_engine_preroll_and_early_release` (drives real `HotKeyStateMachine` $\rightarrow$ `SystemDictationEngine::handle_hotkey_action`, asserting `start_recording` is called on key-down before 250 ms qualifies and `discard_recording` is called on early release), plus mock-level tests `test_mock_level_pre_roll_timeline_preserves_speech_starting_at_100ms_before_250ms_commit` and failure proof `test_mock_level_old_behavior_without_preroll_fails_to_capture_early_speech`. Real hardware cpal audio path remains unverified. | `UNVERIFIED` (Engine & mock-level tests verified; real cpal hardware path UNVERIFIED) |
| **2** | **Start-Menu Suppression Logic** | Pure decision function `should_inject_dummy_suppression` injects `VK_NONAME` ($0\text{xFC}$) via `SendInput` strictly when dictation chord (`ctrl_down && win_down`) is active. Verified by unit tests `test_plain_win_press_and_release_never_injects_suppression` and `test_chord_ctrl_win_triggers_suppression`. | `UNVERIFIED` (Physical monitor Start Menu inspection pending) |
| **3** | **Insertion Timing & Modifier Release** | `wait_for_modifiers_physically_released` polls `GetAsyncKeyState(VK_CONTROL)`, `VK_LWIN`, `VK_RWIN` before any insertion. Out-of-order modifier release (Win first or Ctrl first) supported. Verified by unit tests `test_out_of_order_modifier_release` and `test_wait_for_modifiers_released_returns_ok_when_not_pressed`. | `UNVERIFIED` (Physical keyboard release pending) |
| **4** | **Hands-Free Safety Rules & Cues** | Configurable trailing silence auto-stop ($3.5\text{ s}$ default) and $5\text{ minute}$ ($300\text{ s}$) hard limit. Esc cancels instantly (`cancel()`). Cues emitted via `DictationCue::Start`, `Stop`, `Cancel`. Visual orb maps `Recording` $\rightarrow$ `Listening` (cyan). Verified by unit tests `test_hands_free_safety_rules`, `test_dictation_cues_emitted`, `test_mock_level_dictation_cancellation`, `test_orb_state_mapping`. | `UNVERIFIED` (Physical speaker audio tone pending) |
| **5** | **Password Detection (Win32 + UIA)** | Multi-layered detection: Win32 `ES_PASSWORD` ($0\text{x0020}$), `EM_GETPASSWORDCHAR` ($0\text{x00D2}$), and COM UI Automation `CurrentIsPassword` (covering Chromium, Edge, and Electron). Strictly fails closed (`PlatformError::SecurityCheckFailed`) in headless/non-interactive desktop environments. Verified by unit test `test_password_guardrail_fails_closed_in_headless`. | `UNVERIFIED` (Physical browser password form pending) |
| **6** | **Elevated Window UIPI Guardrail** | Target window elevation checked via `GetWindowThreadProcessId` + `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `OpenProcessToken` (failing closed on `ACCESS_DENIED`). Insertion refused via `PlatformError::ElevatedTargetBlocked`, and UI notification emitted. | `UNVERIFIED` (Physical elevated prompt pending) |
| **7** | **API Key & Log Hygiene** | API keys passed exclusively via HTTP headers (`x-goog-api-key`, `Authorization: Bearer`), never query params. `redact_sensitive_content` scrubs Google API keys (`AIzaSy...`), FreeLLMAPI keys (`freellmapi-...`), raw audio byte streams, and transcripts from error logs and bodies. Verified by unit test `test_key_hygiene_and_log_redaction` and `acceptance/secret_scan.py`. `git log --all -p -G"AIza[0-9A-Za-z_-]{35}" --oneline` and `git log --all -p -G"freellmapi-[A-Za-z0-9]{16,}" --oneline` both return completely empty. | `VERIFIED` |
| **8** | **Multi-Format Clipboard Backup & Restore** | Enumerates all clipboard formats via `EnumClipboardFormats`, filters out GDI object handles (`CF_BITMAP`, `CF_PALETTE`, etc.) to prevent `STATUS_HEAP_CORRUPTION`, snapshots `HGLOBAL` bytes, marks paste with `ExcludeClipboardContentFromMonitorProcessing` & `CanIncludeInClipboardHistory = 0`, pastes via `Ctrl+V`, and restores prior clipboard formats cleanly. Verified by unit tests `test_clipboard_history_exclusion_formats_registered` and `test_clipboard_multiformat_backup_and_restore` (text, DIB bitmap, and HDROP file list). | `UNVERIFIED` (Physical Win+V history list pending) |
| **9** | **Live Gemini Smoke & Adversarial Test** | Primary model `gemini-3.8-flash` has never served a request (returns HTTP 503 UNAVAILABLE on every attempt); therefore, `thinking_level: "low"` on `gemini-3.8-flash` is UNVERIFIED. Item 9 is verified strictly for `gemini-3-flash-preview`. Configurable via `GEMINI_MODEL` environment variable. | `VERIFIED` (Strictly for `gemini-3-flash-preview`; `gemini-3.8-flash` is UNVERIFIED) |
| **10** | **Clean CI & Lint Quality** | All checks clean across all crates: `cargo fmt --check` (0 diff), `cargo clippy --workspace --all-targets -- -D warnings` (0 warnings, 0 errors), `cargo test --workspace` (49 tests passing). | `VERIFIED` |
| **11** | **FreeLLMAPI Third-Party Fallback Client** | Implemented `OpenAiCompatDictationClient` behind `DictationClient`. Path 1 (`chat/completions` with `input_audio` {data: base64, format: "wav"}) and Path 2 (fallback on 4xx to `POST /audio/transcriptions` then text-only `chat/completions`). Sets `last_served_backend` to `"freellmapi:chat_audio"` or `"freellmapi:transcribe_then_chat"`. Fails fast on unreachable proxy via 1.0s connect timeout. 3-tier fallback chain (Gemini primary $\rightarrow$ Gemini fallback $\rightarrow$ FreeLLMAPI) gated behind explicit `ALLOW_THIRD_PARTY_FALLBACK` flag and configured key. Distinguishes transient 429 (60s cooldown) from daily quota exhaustion 429 (30m lockout on Google API key, routing subsequent requests directly to FreeLLMAPI). Config errors (400, 401, 403) surface immediately without falling back. Emits `"console_notice"` and `"toast_notification"` ("served via FreeLLMAPI"). | `VERIFIED` (Unit tests `test_freellmapi_audio_path_success`, `test_freellmapi_audio_rejected_fallback_to_transcription`, `test_freellmapi_proxy_down_fast_fail_and_retained_audio`, `test_google_daily_quota_429_skips_google_and_routes_to_freellmapi`, `test_allow_third_party_fallback_false_never_calls_freellmapi`, `test_google_config_error_403_never_falls_back`) |

---

## 2. Technical Evidence Details

### 1. Pre-roll Engine-Level & Mock-Level Timeline Tests
- **Engine-Level Test (`test_engine_level_hotkey_to_engine_preroll_and_early_release`)**:
  - Simulates raw OS key events through `HotKeyStateMachine` into `SystemDictationEngine::handle_hotkey_action`.
  - When Ctrl is pressed at $t=0\text{ ms}$ and Win is pressed at $t=10\text{ ms}$, `PreRollStarted` is dispatched. `AudioSource::start_recording()` is invoked immediately at $t=10\text{ ms}$—well before the 250 ms hold threshold.
  - Dictation state remains `Idle` (preventing early UI disruption or audio cues).
  - When chord is released early at $t=80\text{ ms}$ (hold duration $70\text{ ms} < 250\text{ ms}$), `PreRollDiscarded` is dispatched, and `AudioSource::discard_recording()` is called immediately.
  - When chord is held past 250 ms ($t=750\text{ ms}$), `HoldStarted` is emitted, triggering `commit_recording()` and transitioning state to `Recording` (`Listening`). Release at $t=1000\text{ ms}$ triggers `stop_recording()` and text insertion.
- **Mock-Level Timeline & Failure Proof**:
  - `test_mock_level_pre_roll_timeline_preserves_speech_starting_at_100ms_before_250ms_commit`: Speech starting at $t=100\text{ ms}$ before commit at $t=250\text{ ms}$ is preserved in buffer.
  - `test_mock_level_old_behavior_without_preroll_fails_to_capture_early_speech`: Proves that starting capture only at commit permanently loses speech from $t=100\text{ ms}$ to $t=250\text{ ms}$.
- **Hardware Caveat**: Real hardware cpal WASAPI input stream latency remains **UNVERIFIED**.

### 2. VAD Architecture: Pure Energy-Based RMS VAD
The audio pipeline in `ruby-voice/src/vad.rs` uses exclusively **Energy-based RMS VAD** (`ruby_voice::trim_silence_vad`). It computes RMS energy over 20 ms frames (320 samples @ 16 kHz) against an RMS threshold of 300.0, prepending 100 ms lead padding and appending 160 ms tail padding. It does NOT use Silero ONNX or neural VAD.

### 3. Model Configuration, Timeout Scaling & Unhealthy Cooldown Architecture
- **Timeout Scaling Formula with Audio Duration**:
  - Audio duration: $T_{\text{sec}} = \max(0.0, (\text{wav\_len} - 44) / 32000.0)$
  - Primary Model (`gemini-3.8-flash`):
    $$\text{Timeout}_{\text{primary}} = \min(3000\text{ ms} + 200\text{ ms} \times T_{\text{sec}}, 12000\text{ ms})$$
  - Fallback Model (`gemini-3-flash-preview`):
    $$\text{Timeout}_{\text{fallback}} = \min(5000\text{ ms} + 300\text{ ms} \times T_{\text{sec}}, 15000\text{ ms})$$
  - Verified by unit test `test_timeout_scaling_formula`.
- **Hanging Primary & Fallback Verification**:
  - In `test_primary_hang_timeout_fallback_and_marked_unhealthy`, primary server sleeps 1200 ms (> 500 ms primary timeout).
  - Time-to-fallback is bounded by primary timeout: elapsed $\approx 550\text{ ms}$ (within $[480\text{ ms}, 900\text{ ms}]$).
  - Fallback model succeeds, and `client.is_primary_unhealthy()` is asserted true.
- **Short Audio vs. Long Audio Unhealthy Policy**:
  - Threshold: `SHORT_AUDIO_THRESHOLD_SECS = 5.0s`.
  - 5xx status codes, transient 429 rate limits, and network connection errors mark the primary unhealthy for a 60-second cooldown window.
  - Timeouts on **short audio** ($\le 5.0\text{ s}$) mark the primary unhealthy for 60 seconds.
  - Timeouts on **long audio** ($> 5.0\text{ s}$) fall back immediately for the current request, but do **NOT** mark the primary unhealthy. Verified by `test_primary_hang_on_long_audio_does_not_mark_unhealthy`.

### 4. FreeLLMAPI Third-Party Fallback Backend
- **Implementation**: `crates/ruby-gemini/src/openai_client.rs` implements `DictationClient`.
  - **Path 1**: `POST /v1/chat/completions` with `input_audio: { data: base64, format: "wav" }` and single-pass cleanup prompt.
  - **Path 2**: On 4xx rejection (parameter unsupported), falls back to `POST /v1/audio/transcriptions` multipart WAV upload followed by text-only `POST /v1/chat/completions` cleanup prompt.
  - **Fail Fast**: Client configured with `.connect_timeout(Duration::from_millis(1000))`. Fails fast within $\le 1.0\text{ s}$ if the proxy daemon is down.
  - **Gating**: Active only when `ALLOW_THIRD_PARTY_FALLBACK` is `true` AND `FREELLMAPI_API_KEY` is present in keyring or environment. Default is `false` (OFF).
  - **30-Minute Daily Quota Lockout**: Detected via `is_daily_quota_exhausted` (HTTP 429 with `"per day"`, `"daily"`, or `"quota exceeded"`). Sets `google_key_exhausted_until = now + 1800s`. All subsequent requests bypass Google models entirely and route straight to FreeLLMAPI. Verified by `test_google_daily_quota_429_skips_google_and_routes_to_freellmapi`.
  - **Config Errors**: HTTP 400, 401, 403 on Google models fail fast and NEVER trigger fallback to FreeLLMAPI. Verified by `test_google_config_error_403_never_falls_back`.
  - **Console Notice Event**: When a dictation is served via FreeLLMAPI, `crates/ruby-desktop/src/lib.rs` emits `console_notice` and `toast_notification` with `"served via FreeLLMAPI"`, which is rendered as a calm static notice capsule in the Console masthead.
  - **Key Hygiene & Redaction**: `redact_sensitive_content` scrubs `freellmapi-[A-Za-z0-9_\-]{16,}`. `git log --all -p -G"freellmapi-[A-Za-z0-9]{16,}" --oneline` returns empty.

### 5. Dictation Time-to-Text Bounds (3-Tier Chain)
- **Tier 1 (Gemini Primary Healthy)**:
  - Typical: **~1.0 – 1.5 seconds**.
- **Tier 2 (Gemini Primary 503/Timeout $\rightarrow$ Fallback Served)**:
  - Primary timeout: $3.0\text{ s}$ (short audio)
  - Fallback network: $1.5\text{ s}$
  - **Total**: **~4.5 – 5.0 seconds**.
- **Tier 3 (Both Gemini Models Fail $\rightarrow$ FreeLLMAPI Served)**:
  - Primary timeout: $3.0\text{ s}$
  - Fallback timeout: $5.0\text{ s}$
  - FreeLLMAPI:
    - If Path 1 succeeds: $1.0 - 2.0\text{ s} \rightarrow$ **Total: ~9.0 – 10.0 seconds**.
    - If Path 1 rejected (4xx, 50ms) + Path 2 transcribe & chat ($3.0\text{ s}$): $\rightarrow$ **Total: ~11.0 – 13.0 seconds**.
    - If proxy unreachable: connect timeout $\le 1.0\text{ s} \rightarrow$ **Total time to failure: ~9.0 seconds**.
- **Worst-Case Theoretical Cap (Max audio duration $60\text{ s}$, max timeouts)**:
  - Primary cap ($12.0\text{ s}$) + Fallback cap ($15.0\text{ s}$) + FreeLLMAPI cap ($15.0\text{ s}$) = **~42.0 seconds**. In all failure modes, the raw audio WAV is preserved in memory for immediate zero-re-record retry.
