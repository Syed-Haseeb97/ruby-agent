# Milestone M2 Acceptance Report: Conversational Chat, Emotion Expression & Speech Synthesis

**Repository**: `Syed-Haseeb97/ruby-agent`  
**Platform**: Windows 11 (Tauri 2.0 Native Shell)  
**Verification Date**: 2026-11-18  

---

## 1. Executive Summary

Milestone M2 establishes Ruby's multi-turn conversational core, emotional expressions, and multi-engine text-to-speech (TTS) synthesis, replacing early heuristic stubs with the complete architecture from the original PyQt6 / LiquidBlob implementation:
1. **Multi-Turn Chat**: Structured response extraction separating user-facing dialogue from emotion labels (`{response, emotion}`), robust markdown-fence unwrapping, inner JSON detection, and safe Neutral fallback.
2. **First-Person Distress Mapping**: 8 regex distress patterns mapping disclosures of distress (e.g. "I had a rough day", "I'm overwhelmed") directly to `Empathetic`, preventing inappropriate `Sad` or mechanical mirroring.
3. **Multi-Engine TTS Fallback Chain**: Prioritized chain (`Edge-TTS` -> `Gemini-TTS` -> `Windows-Speech`) with per-engine timeouts and failover.
4. **Non-Blocking Cancellable Playback**: Audio playback runs asynchronously without stalling the UI thread; cancellation (via Esc key or new input) immediately cuts off speech.
5. **Emotion-Driven Orb**: When speech begins, the orb displays the classified emotion (`Happy`, `Excited`, `Sad`, `Empathetic`, `Curious`, `Surprised`) with active lip/mouth sync, returning smoothly to `Idle` / `Neutral` upon completion.
6. **Log Privacy Hygiene**: Transcripts, user prompts, and model responses are strictly excluded from logs.

---

## 2. Verification Matrix

| Spec Item | Description | Status | Verification Evidence |
|---|---|---|---|
| **B1.1** | `ChatSession` multi-turn history accumulation | **VERIFIED** | Unit test `chat::tests::test_chat_session_accumulates_turns` |
| **B1.2** | Structured JSON parsing `{response, emotion}` | **VERIFIED** | Unit test `chat::tests::test_parses_response_and_supported_emotion` |
| **B1.3** | Safe Neutral fallback on missing/malformed JSON | **VERIFIED** | Unit tests `test_invalid_emotion_falls_back_to_neutral`, `test_malformed_json_is_kept_as_plain_text` |
| **B1.4** | Distress cue mapping to `Empathetic` | **VERIFIED** | Unit tests `test_bad_day_maps_to_empathetic`, `test_supportive_disclosures_map_to_empathetic` |
| **B1.5** | Negative words in queries do not trigger false empathy | **VERIFIED** | Unit test `test_unrelated_negative_word_does_not_force_empathy` |
| **B1.6** | Emotion label never spoken or typed | **VERIFIED** | Unit test `test_emotion_label_never_spoken_or_typed` |
| **B1.7** | Primary 60s unhealthy cache & length-scaled timeout | **VERIFIED** | Unit test `chat_client::tests::test_timeout_scaling_chat` & `dictation_client::tests` |
| **B1.8** | FreeLLMAPI third-party fallback gating | **VERIFIED** | Enforced via `allow_third_party_fallback` configuration flag |
| **B1.9** | Centralized system prompt | **VERIFIED** | `ruby_config::load_system_prompt()` in `crates/ruby-config/src/lib.rs` |
| **B2.1** | `TtsEngine` trait abstraction | **VERIFIED** | `EdgeTtsEngine`, `GeminiTtsEngine`, `WindowsSpeechEngine`, `MockTtsEngine` in `crates/ruby-core/src/tts.rs` |
| **B2.2** | Automatic TTS fallback chain | **VERIFIED** | Unit test `tts::tests::test_fallback_chain_falls_back_when_primary_fails` |
| **B2.3** | Non-blocking cancellable playback | **VERIFIED** | Unit test `tts::tests::test_playback_controller_cancel` |
| **B2.4** | Real hardware audio emission | **UNVERIFIED** | Requires live audio output hardware observation on Windows 11 host |
| **B3.1** | 7 emotion looks + Thinking + Error in Canvas 2D | **VERIFIED** | `LiquidOrbCanvas.tsx` with mathematical parity to `ui/liquid_blob.py` |
| **B3.2** | Emotion drives speaking orb, returns to Neutral/Idle | **VERIFIED** | Handled in `commands::send_chat_message` lifecycle hooks |
| **B4.1** | Editorial Console chat view | **VERIFIED** | `ConsoleView.tsx` with `#F5F3EE` paper palette and static text pills |
| **B4.2** | Zero pulsing or blinking indicators | **VERIFIED** | Confirmed by code audit: zero `animate-pulse` or blinking timers in Console |
| **B5.1** | End-to-end Tauri wiring (Chat -> Emotion -> TTS -> Orb) | **VERIFIED** | Handled via Tauri IPC `send_chat_message` and `cancel_speech` |
