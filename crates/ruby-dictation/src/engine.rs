//! End-to-end System Dictation Engine.
//! Coordinates: Audio capture -> VAD -> 16kHz WAV encode -> Gemini cleanup -> Text insertion.

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info};

use ruby_gemini::DictationClient;
use ruby_platform::DesktopController;
use ruby_voice::{encode_wav_16k_mono, trim_silence_vad, AudioSource};

use crate::{
    DictationCue, DictationError, DictationLatencyBudget, DictationState, HandsFreeAction,
    HandsFreeConfig,
};

pub struct SystemDictationEngine {
    audio_source: Arc<Mutex<Box<dyn AudioSource>>>,
    dictation_client: Arc<Box<dyn DictationClient>>,
    desktop_controller: Arc<Box<dyn DesktopController>>,
    state: Arc<RwLock<DictationState>>,
    last_audio_wav: Arc<Mutex<Option<Vec<u8>>>>,
    last_target_process: Arc<Mutex<Option<String>>>,
    on_state_change: Option<Arc<dyn Fn(DictationState) + Send + Sync>>,
    on_cue: Option<Arc<dyn Fn(DictationCue) + Send + Sync>>,
    hands_free_config: HandsFreeConfig,
}

impl SystemDictationEngine {
    pub fn new(
        audio_source: Box<dyn AudioSource>,
        dictation_client: Box<dyn DictationClient>,
        desktop_controller: Box<dyn DesktopController>,
        on_state_change: Option<Arc<dyn Fn(DictationState) + Send + Sync>>,
    ) -> Self {
        Self::with_config(
            audio_source,
            dictation_client,
            desktop_controller,
            on_state_change,
            None,
            HandsFreeConfig::default(),
        )
    }

    pub fn with_config(
        audio_source: Box<dyn AudioSource>,
        dictation_client: Box<dyn DictationClient>,
        desktop_controller: Box<dyn DesktopController>,
        on_state_change: Option<Arc<dyn Fn(DictationState) + Send + Sync>>,
        on_cue: Option<Arc<dyn Fn(DictationCue) + Send + Sync>>,
        hands_free_config: HandsFreeConfig,
    ) -> Self {
        Self {
            audio_source: Arc::new(Mutex::new(audio_source)),
            dictation_client: Arc::new(dictation_client),
            desktop_controller: Arc::new(desktop_controller),
            state: Arc::new(RwLock::new(DictationState::Idle)),
            last_audio_wav: Arc::new(Mutex::new(None)),
            last_target_process: Arc::new(Mutex::new(None)),
            on_state_change,
            on_cue,
            hands_free_config,
        }
    }

    fn emit_cue(&self, cue: DictationCue) {
        if let Some(ref cb) = self.on_cue {
            cb(cue);
        }
    }

    pub fn hands_free_config(&self) -> &HandsFreeConfig {
        &self.hands_free_config
    }

    /// Evaluates hands-free safety rules:
    /// - 5-minute (300s) hard limit: auto-stop and submit
    /// - 3.5s of sustained trailing silence: auto-stop and submit
    pub fn evaluate_hands_free_safety(
        &self,
        elapsed_recording_ms: u64,
        trailing_silence_ms: u64,
    ) -> HandsFreeAction {
        if elapsed_recording_ms >= self.hands_free_config.max_recording_duration_ms {
            HandsFreeAction::AutoStopMaxDuration
        } else if trailing_silence_ms >= self.hands_free_config.silence_auto_stop_duration_ms {
            HandsFreeAction::AutoStopSilence
        } else {
            HandsFreeAction::Continue
        }
    }

    async fn set_state(&self, new_state: DictationState) {
        let mut s = self.state.write().await;
        *s = new_state;
        if let Some(ref cb) = self.on_state_change {
            cb(new_state);
        }
    }

    pub async fn current_state(&self) -> DictationState {
        *self.state.read().await
    }

    /// Starts pre-roll audio buffering immediately on Ctrl+Win key-down.
    /// Dictation state remains Idle so no UI change or recording cue occurs until committed.
    pub async fn start_pre_roll(&self) -> Result<(), DictationError> {
        let mut audio = self.audio_source.lock().await;
        audio
            .start_recording()
            .map_err(|e| DictationError::Capture(e.to_string()))?;
        info!("Pre-roll audio capture started into rolling ring buffer");
        Ok(())
    }

    /// Commits active recording once hold passes ~250 ms (or hands-free mode engaged).
    /// Transitions state to Recording (Orb -> Listening) and emits start cue.
    pub async fn commit_pre_roll(&self) -> Result<(), DictationError> {
        {
            let mut audio = self.audio_source.lock().await;
            audio
                .commit_recording()
                .map_err(|e| DictationError::Capture(e.to_string()))?;
        }
        self.set_state(DictationState::Recording).await;
        self.emit_cue(DictationCue::Start);
        info!("Pre-roll buffer committed to active Recording (Orb: Listening)");
        Ok(())
    }

    /// Discards pre-roll audio buffer on early release (<250 ms tap).
    pub async fn discard_pre_roll(&self) -> Result<(), DictationError> {
        {
            let mut audio = self.audio_source.lock().await;
            audio
                .discard_recording()
                .map_err(|e| DictationError::Capture(e.to_string()))?;
        }
        self.set_state(DictationState::Idle).await;
        info!("Pre-roll buffer discarded on early release (<250ms)");
        Ok(())
    }

    /// Legacy / direct start recording. Commits immediately.
    pub async fn start_recording(&self) -> Result<(), DictationError> {
        self.start_pre_roll().await?;
        self.commit_pre_roll().await?;
        Ok(())
    }

    /// Stops audio capture, trims silence, encodes WAV, calls Gemini, and injects text.
    pub async fn finish_and_insert(
        &self,
        foreground_process: Option<String>,
    ) -> Result<DictationLatencyBudget, DictationError> {
        let t_total_start = Instant::now();

        // 1. Stop audio capture
        let raw_samples = {
            let mut audio = self.audio_source.lock().await;
            audio
                .stop_recording()
                .map_err(|e| DictationError::Capture(e.to_string()))?
        };
        self.emit_cue(DictationCue::Stop);

        if raw_samples.is_empty() {
            self.set_state(DictationState::Idle).await;
            return Ok(DictationLatencyBudget {
                vad_millis: 0,
                encode_millis: 0,
                network_model_millis: 0,
                insertion_millis: 0,
                total_millis: t_total_start.elapsed().as_millis() as u64,
            });
        }

        // 2. Local Energy VAD
        let t_vad_start = Instant::now();
        let trimmed_samples = trim_silence_vad(&raw_samples, 16000, 300.0);
        let vad_millis = t_vad_start.elapsed().as_millis() as u64;

        if trimmed_samples.is_empty() {
            info!("VAD detected only silence; discarding audio without API call");
            self.set_state(DictationState::Idle).await;
            return Ok(DictationLatencyBudget {
                vad_millis,
                encode_millis: 0,
                network_model_millis: 0,
                insertion_millis: 0,
                total_millis: t_total_start.elapsed().as_millis() as u64,
            });
        }

        // 3. Encode to 16 kHz mono WAV
        let t_enc_start = Instant::now();
        let wav_bytes = encode_wav_16k_mono(trimmed_samples);
        let encode_millis = t_enc_start.elapsed().as_millis() as u64;

        // Retain latest audio buffer in memory in case network retry is needed
        {
            let mut last_wav = self.last_audio_wav.lock().await;
            *last_wav = Some(wav_bytes.clone());
            let mut last_proc = self.last_target_process.lock().await;
            *last_proc = foreground_process.clone();
        }

        // 4. Multimodal Gemini 3.8 Flash STT & Cleanup
        self.set_state(DictationState::Processing).await;
        let t_model_start = Instant::now();

        let proc_ref = foreground_process.as_deref();
        let transcribed_text = match self
            .dictation_client
            .transcribe_and_cleanup(&wav_bytes, proc_ref)
            .await
        {
            Ok(text) => text,
            Err(e) => {
                error!("Gemini dictation API failed: {:?}", e);
                self.set_state(DictationState::Error).await;
                return Err(DictationError::Model(e.to_string()));
            }
        };

        let network_model_millis = t_model_start.elapsed().as_millis() as u64;

        // 5. Text Insertion via DesktopController
        let t_ins_start = Instant::now();
        if !transcribed_text.is_empty() {
            self.set_state(DictationState::Inserting).await;
            if let Err(e) = self.desktop_controller.type_text(&transcribed_text) {
                error!("Text insertion failed: {:?}", e);
                self.set_state(DictationState::Error).await;
                return Err(DictationError::Insertion(e.to_string()));
            }
        }
        let insertion_millis = t_ins_start.elapsed().as_millis() as u64;

        let total_millis = t_total_start.elapsed().as_millis() as u64;
        self.set_state(DictationState::Idle).await;

        let budget = DictationLatencyBudget {
            vad_millis,
            encode_millis,
            network_model_millis,
            insertion_millis,
            total_millis,
        };

        info!(
            vad_ms = budget.vad_millis,
            encode_ms = budget.encode_millis,
            model_ms = budget.network_model_millis,
            insert_ms = budget.insertion_millis,
            total_ms = budget.total_millis,
            within_target = budget.is_within_target(),
            "Dictation pipeline completed successfully"
        );

        Ok(budget)
    }

    /// Retries the last failed dictation using retained in-memory audio buffer without re-recording.
    pub async fn retry_last_dictation(&self) -> Result<DictationLatencyBudget, DictationError> {
        let (wav_bytes, proc_name) = {
            let wav_guard = self.last_audio_wav.lock().await;
            let proc_guard = self.last_target_process.lock().await;
            let wav_bytes = match wav_guard.as_ref() {
                Some(w) => w.clone(),
                None => {
                    return Err(DictationError::Model(
                        "No retained audio buffer to retry".to_string(),
                    ))
                }
            };
            let proc_name = proc_guard.clone();
            (wav_bytes, proc_name)
        };

        let t_total_start = Instant::now();
        self.set_state(DictationState::Processing).await;

        let t_model_start = Instant::now();
        let transcribed_text = match self
            .dictation_client
            .transcribe_and_cleanup(&wav_bytes, proc_name.as_deref())
            .await
        {
            Ok(text) => text,
            Err(e) => {
                error!("Gemini dictation API retry failed: {:?}", e);
                self.set_state(DictationState::Error).await;
                return Err(DictationError::Model(e.to_string()));
            }
        };
        let network_model_millis = t_model_start.elapsed().as_millis() as u64;

        let t_ins_start = Instant::now();
        if !transcribed_text.is_empty() {
            self.set_state(DictationState::Inserting).await;
            if let Err(e) = self.desktop_controller.type_text(&transcribed_text) {
                self.set_state(DictationState::Error).await;
                return Err(DictationError::Insertion(e.to_string()));
            }
        }
        let insertion_millis = t_ins_start.elapsed().as_millis() as u64;

        let total_millis = t_total_start.elapsed().as_millis() as u64;
        self.set_state(DictationState::Idle).await;

        Ok(DictationLatencyBudget {
            vad_millis: 0,
            encode_millis: 0,
            network_model_millis,
            insertion_millis,
            total_millis,
        })
    }

    /// Cancels active recording and resets state to Idle.
    pub async fn cancel(&self) {
        let mut audio = self.audio_source.lock().await;
        let _ = audio.stop_recording();
        self.set_state(DictationState::Idle).await;
        self.emit_cue(DictationCue::Cancel);
        info!("Dictation cancelled");
    }

    /// Returns true if an in-memory audio buffer is retained for retry.
    pub async fn has_retained_audio(&self) -> bool {
        self.last_audio_wav.lock().await.is_some()
    }

    /// Dispatches a high-level HotKeyAction directly into the dictation engine pipeline.
    pub async fn handle_hotkey_action(
        &self,
        action: ruby_platform::HotKeyAction,
        foreground_process: Option<String>,
    ) -> Result<(), DictationError> {
        match action {
            ruby_platform::HotKeyAction::PreRollStarted => {
                self.start_pre_roll().await?;
            }
            ruby_platform::HotKeyAction::PreRollDiscarded => {
                self.discard_pre_roll().await?;
            }
            ruby_platform::HotKeyAction::HoldStarted => {
                self.commit_pre_roll().await?;
            }
            ruby_platform::HotKeyAction::HoldReleased { .. } => {
                let _ = self.finish_and_insert(foreground_process).await?;
            }
            ruby_platform::HotKeyAction::DoubleTapToggled { active } => {
                if active {
                    self.start_recording().await?;
                } else {
                    let _ = self.finish_and_insert(foreground_process).await?;
                }
            }
            ruby_platform::HotKeyAction::Cancelled
            | ruby_platform::HotKeyAction::AbortedThirdKey { .. } => {
                self.cancel().await;
            }
            ruby_platform::HotKeyAction::KillSwitchTriggered => {
                self.cancel().await;
            }
        }
        Ok(())
    }
}
