//! In-memory telemetry & performance metrics collector for Ruby.
//! Tracks dictation latency breakdown, model fallback reasons, background jobs,
//! computer use actions, and remote commands. Formats rich status for Console & Telegram.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DictationLatencyMetrics {
    pub audio_len_ms: u64,
    pub upload_ms: u64,
    pub inference_ms: u64,
    pub typing_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelFallbackMetrics {
    pub timeout: u64,
    pub rate_limit: u64,
    pub server_error: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackgroundJobMetrics {
    pub active: usize,
    pub completed: usize,
    pub failed: usize,
    pub total_duration_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComputerUseMetrics {
    pub actions_total: usize,
    pub actions_confirmed: usize,
    pub actions_denied: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteMetrics {
    pub commands_total: usize,
    pub active_sessions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemMetricsSnapshot {
    pub uptime_secs: u64,
    pub dictation: DictationLatencyMetrics,
    pub fallbacks: ModelFallbackMetrics,
    pub background_jobs: BackgroundJobMetrics,
    pub computer_use: ComputerUseMetrics,
    pub remote: RemoteMetrics,
}

#[derive(Clone)]
pub struct MetricsCollector {
    start_time: Instant,
    inner: Arc<Mutex<SystemMetricsSnapshot>>,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            inner: Arc::new(Mutex::new(SystemMetricsSnapshot {
                uptime_secs: 0,
                dictation: DictationLatencyMetrics::default(),
                fallbacks: ModelFallbackMetrics::default(),
                background_jobs: BackgroundJobMetrics::default(),
                computer_use: ComputerUseMetrics::default(),
                remote: RemoteMetrics::default(),
            })),
        }
    }

    /// Record completed dictation latency metrics
    pub fn record_dictation_latency(
        &self,
        audio_len_ms: u64,
        upload_ms: u64,
        inference_ms: u64,
        typing_ms: u64,
    ) {
        let mut m = self.inner.lock().unwrap();
        m.dictation.audio_len_ms = audio_len_ms;
        m.dictation.upload_ms = upload_ms;
        m.dictation.inference_ms = inference_ms;
        m.dictation.typing_ms = typing_ms;
        m.dictation.total_ms = upload_ms + inference_ms + typing_ms;
    }

    /// Record model fallback event
    pub fn record_fallback(&self, reason: &str) {
        let mut m = self.inner.lock().unwrap();
        match reason.to_lowercase().as_str() {
            "timeout" => m.fallbacks.timeout += 1,
            "rate_limit" => m.fallbacks.rate_limit += 1,
            _ => m.fallbacks.server_error += 1,
        }
    }

    /// Update background job metrics
    pub fn update_jobs(&self, active: usize, completed: usize, failed: usize, duration_ms: u64) {
        let mut m = self.inner.lock().unwrap();
        m.background_jobs.active = active;
        m.background_jobs.completed = completed;
        m.background_jobs.failed = failed;
        m.background_jobs.total_duration_ms += duration_ms;
    }

    /// Record Computer Use action outcome
    pub fn record_computer_use_action(&self, outcome: &str) {
        let mut m = self.inner.lock().unwrap();
        m.computer_use.actions_total += 1;
        match outcome.to_lowercase().as_str() {
            "confirmed" => m.computer_use.actions_confirmed += 1,
            "denied" => m.computer_use.actions_denied += 1,
            _ => {}
        }
    }

    /// Record remote Telegram command
    pub fn record_remote_command(&self) {
        let mut m = self.inner.lock().unwrap();
        m.remote.commands_total += 1;
    }

    /// Get instantaneous metrics snapshot
    pub fn get_snapshot(&self) -> SystemMetricsSnapshot {
        let mut snap = self.inner.lock().unwrap().clone();
        snap.uptime_secs = self.start_time.elapsed().as_secs();
        snap
    }

    /// Formats a rich, human-readable status report suitable for Telegram `/status` and Console
    pub fn format_status_report(&self) -> String {
        let s = self.get_snapshot();
        format!(
            "📊 **Ruby System Status & Health**\n\
            • Uptime: {}s\n\
            • Dictation Latency (last): {} ms (audio: {}ms, api: {}ms, typing: {}ms)\n\
            • Model Fallbacks: timeout={}, 429_rate_limit={}, server_err={}\n\
            • Background Jobs: active={}, completed={}, failed={}\n\
            • Computer Use: total={}, confirmed={}, denied={}\n\
            • Remote Telegram: commands_processed={}",
            s.uptime_secs,
            s.dictation.total_ms,
            s.dictation.audio_len_ms,
            s.dictation.inference_ms,
            s.dictation.typing_ms,
            s.fallbacks.timeout,
            s.fallbacks.rate_limit,
            s.fallbacks.server_error,
            s.background_jobs.active,
            s.background_jobs.completed,
            s.background_jobs.failed,
            s.computer_use.actions_total,
            s.computer_use.actions_confirmed,
            s.computer_use.actions_denied,
            s.remote.commands_total
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector_record_and_snapshot() {
        let collector = MetricsCollector::new();

        collector.record_dictation_latency(2500, 120, 350, 40);
        collector.record_fallback("timeout");
        collector.record_fallback("rate_limit");
        collector.update_jobs(1, 4, 0, 1200);
        collector.record_computer_use_action("confirmed");
        collector.record_computer_use_action("denied");
        collector.record_remote_command();

        let snap = collector.get_snapshot();
        assert_eq!(snap.dictation.total_ms, 510);
        assert_eq!(snap.fallbacks.timeout, 1);
        assert_eq!(snap.fallbacks.rate_limit, 1);
        assert_eq!(snap.background_jobs.completed, 4);
        assert_eq!(snap.computer_use.actions_total, 2);
        assert_eq!(snap.computer_use.actions_confirmed, 1);
        assert_eq!(snap.computer_use.actions_denied, 1);
        assert_eq!(snap.remote.commands_total, 1);

        let report = collector.format_status_report();
        assert!(report.contains("Ruby System Status & Health"));
        assert!(report.contains("Dictation Latency"));
        assert!(report.contains("timeout=1"));
    }
}
