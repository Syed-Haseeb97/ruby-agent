//! Append-only local security audit log.
//! Records: timestamp, sender_id, task_id, command, outcome.
//! STRICT SECURITY RULE: Never logs message bodies from unknown senders, never logs tokens.

use chrono::Utc;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AuditLogger {
    log_path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl AuditLogger {
    pub fn new(log_path: PathBuf) -> Self {
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self {
            log_path,
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn default_path() -> PathBuf {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| "D:\\projects\\ruby-agent".to_string());
        Path::new(&home).join(".ruby").join("remote_audit.log")
    }

    /// Records an audit log entry.
    pub fn log_entry(
        &self,
        sender_id: i64,
        task_id: Option<&str>,
        command_type: &str,
        outcome: &str,
    ) {
        let _guard = self.lock.lock().ok();
        let timestamp = Utc::now().to_rfc3339();
        let task_str = task_id.unwrap_or("-");

        let entry = format!(
            "[{}] sender={} task={} cmd={} outcome={}\n",
            timestamp, sender_id, task_str, command_type, outcome
        );

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
        {
            let _ = file.write_all(entry.as_bytes());
        }
    }
}
