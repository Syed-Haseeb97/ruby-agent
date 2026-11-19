//! Structured JSON-lines logging with daily rotation, 14-day retention, and global secret redaction.
//! All log entries are written to %LOCALAPPDATA%\Ruby\logs\ruby-YYYY-MM-DD.log.
//! Secrets matching S10 patterns or [SECRET] tags are replaced with [REDACTED] before disk write.

use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub const DEFAULT_LOG_RETENTION_DAYS: i64 = 14;
pub const RECENT_LOG_BUFFER_CAPACITY: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

#[derive(Clone)]
pub struct RedactedJsonLogger {
    log_dir: PathBuf,
    recent_buffer: Arc<Mutex<VecDeque<LogEntry>>>,
    patterns: Arc<Vec<Regex>>,
}

impl RedactedJsonLogger {
    /// Initializes the logger, ensures the log directory exists, and prunes logs older than retention days.
    pub fn new(log_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&log_dir);
        let logger = Self {
            log_dir: log_dir.clone(),
            recent_buffer: Arc::new(Mutex::new(VecDeque::with_capacity(RECENT_LOG_BUFFER_CAPACITY))),
            patterns: Arc::new(Self::compile_redaction_patterns()),
        };
        logger.prune_old_logs(DEFAULT_LOG_RETENTION_DAYS);
        logger
    }

    fn compile_redaction_patterns() -> Vec<Regex> {
        vec![
            // Google API Key
            Regex::new(r"AIza[0-9A-Za-z-_]{35}").unwrap(),
            // OpenAI / Anthropic Secret Key
            Regex::new(r"sk-[a-zA-Z0-9]{20,}").unwrap(),
            // FreeLLMAPI Key
            Regex::new(r"freellmapi-[A-Za-z0-9_\-]{16,}").unwrap(),
            // Telegram Bot Token
            Regex::new(r"[0-9]{8,10}:[A-Za-z0-9_-]{35}").unwrap(),
            // GitHub Personal Access Token
            Regex::new(r"ghp_[a-zA-Z0-9]{20,}").unwrap(),
            // GitHub Fine-grained PAT
            Regex::new(r"github_pat_[a-zA-Z0-9_]{20,}").unwrap(),
            // Private Key Header
            Regex::new(r"BEGIN (?:RSA|OPENSSH|DSA|EC|PGP) PRIVATE KEY").unwrap(),
            // AWS Access Key ID
            Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
            // Tagged [SECRET]...[/SECRET]
            Regex::new(r"\[SECRET\].*?\[/SECRET\]").unwrap(),
            // Hardcoded generic secret assignments
            Regex::new(r#"(?i)(?:api_key|apikey|secret|token)\s*[:=]\s*['"][A-Za-z0-9_\-]{16,}['"]"#).unwrap(),
        ]
    }

    /// Redacts all sensitive credentials, keys, and tagged secrets from raw text.
    pub fn sanitize(&self, text: &str) -> String {
        // Strip zero-width evasion characters before applying redaction patterns
        let filtered: String = text
            .chars()
            .filter(|c| !matches!(*c, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}'))
            .collect();
        let mut sanitized = filtered;
        for pattern in self.patterns.iter() {
            sanitized = pattern.replace_all(&sanitized, "[REDACTED]").to_string();
        }
        sanitized
    }

    /// Writes a redacted structured log entry to the daily rotating log file and in-memory ring buffer.
    pub fn log(&self, level: &str, target: &str, message: &str) -> LogEntry {
        let clean_message = self.sanitize(message);
        let now = Utc::now();
        let entry = LogEntry {
            timestamp: now.to_rfc3339(),
            level: level.to_uppercase(),
            target: target.to_string(),
            message: clean_message,
        };

        // Write to in-memory buffer
        {
            let mut buf = self.recent_buffer.lock().unwrap();
            if buf.len() >= RECENT_LOG_BUFFER_CAPACITY {
                buf.pop_front();
            }
            buf.push_back(entry.clone());
        }

        // Daily rotating file path: ruby-YYYY-MM-DD.log
        let filename = format!("ruby-{}.log", now.format("%Y-%m-%d"));
        let file_path = self.log_dir.join(filename);

        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&file_path) {
            if let Ok(json_line) = serde_json::to_string(&entry) {
                let _ = writeln!(file, "{}", json_line);
            }
        }

        entry
    }

    /// Retrieves up to `limit` recent log entries, optionally filtered by level (e.g. "ERROR", "WARN", "INFO").
    pub fn get_recent_logs(&self, level_filter: Option<&str>, limit: usize) -> Vec<LogEntry> {
        let buf = self.recent_buffer.lock().unwrap();
        let filter = level_filter.map(|l| l.to_uppercase());

        buf.iter()
            .rev()
            .filter(|e| {
                if let Some(ref l) = filter {
                    e.level == *l
                } else {
                    true
                }
            })
            .take(limit)
            .cloned()
            .collect()
    }

    /// Prunes log files in the directory older than `max_age_days`.
    pub fn prune_old_logs(&self, max_age_days: i64) -> usize {
        let mut pruned = 0;
        let entries = match fs::read_dir(&self.log_dir) {
            Ok(e) => e,
            Err(_) => return 0,
        };

        let now = Utc::now();
        let max_age_duration = chrono::Duration::days(max_age_days);

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            if !file_name.starts_with("ruby-") || !file_name.ends_with(".log") {
                continue;
            }

            // Extract date part: ruby-YYYY-MM-DD.log
            let date_str = file_name
                .trim_start_matches("ruby-")
                .trim_end_matches(".log");

            if let Ok(file_date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                let file_datetime = file_date.and_hms_opt(0, 0, 0).unwrap().and_utc();
                if now.signed_duration_since(file_datetime) > max_age_duration {
                    let _ = fs::remove_file(&path);
                    pruned += 1;
                }
            }
        }
        pruned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_redaction_patterns_replace_all_s10_secrets() {
        let temp = tempdir().unwrap();
        let logger = RedactedJsonLogger::new(temp.path().to_path_buf());

        // Construct fake secrets via concatenation to avoid matching static repository scanners
        let fake_ghp = format!("{}{}", "ghp_", "1234567890abcdefghijklmnopqrstuvwxyz");
        let fake_pat = format!("{}{}", "github_pat_", "11ABCD_1234567890abcdefghijklmnopqrstuvwxyz");
        let fake_aiza = format!("{}{}", "AIza", "SyD1234567890abcdefghijklmnopqrstuv");
        let fake_sk = format!("{}{}", "sk-", "1234567890abcdefghijklmnopqrstuv");
        let fake_tg = format!("{}{}", "123456789:", "ABCDefghIJKLmnopQRSTuvwxyz123456789");
        let fake_priv = format!("{} {} {}", "BEGIN", "RSA", "PRIVATE KEY");
        let fake_tagged = "[SECRET]super-confidential-user-credential[/SECRET]";

        let input = format!(
            "Tokens: ghp={}, pat={}, aiza={}, sk={}, tg={}, key={}, tagged={}",
            fake_ghp, fake_pat, fake_aiza, fake_sk, fake_tg, fake_priv, fake_tagged
        );

        let sanitized = logger.sanitize(&input);
        assert!(!sanitized.contains(&fake_ghp));
        assert!(!sanitized.contains(&fake_pat));
        assert!(!sanitized.contains(&fake_aiza));
        assert!(!sanitized.contains(&fake_sk));
        assert!(!sanitized.contains(&fake_tg));
        assert!(!sanitized.contains(&fake_priv));
        assert!(!sanitized.contains("super-confidential-user-credential"));
        assert!(sanitized.contains("[REDACTED]"));
    }

    #[test]
    fn test_daily_rotating_log_file_and_retention_pruning() {
        let temp = tempdir().unwrap();
        let logger = RedactedJsonLogger::new(temp.path().to_path_buf());

        let entry = logger.log("INFO", "test_core", "System initialized cleanly.");
        assert_eq!(entry.level, "INFO");
        assert_eq!(entry.message, "System initialized cleanly.");

        let logs = logger.get_recent_logs(None, 10);
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, "System initialized cleanly.");

        // Create an old log file (20 days ago) and a recent one (2 days ago)
        let old_date = (Utc::now() - chrono::Duration::days(20)).format("%Y-%m-%d").to_string();
        let recent_date = (Utc::now() - chrono::Duration::days(2)).format("%Y-%m-%d").to_string();

        let old_file = temp.path().join(format!("ruby-{}.log", old_date));
        let recent_file = temp.path().join(format!("ruby-{}.log", recent_date));

        fs::write(&old_file, "old log line").unwrap();
        fs::write(&recent_file, "recent log line").unwrap();

        assert!(old_file.exists());
        assert!(recent_file.exists());

        let pruned = logger.prune_old_logs(14);
        assert_eq!(pruned, 1);
        assert!(!old_file.exists(), "Old log file must be pruned");
        assert!(recent_file.exists(), "Recent log file must remain intact");
    }

    #[test]
    fn test_randomized_property_redaction_fuzzing() {
        let temp = tempdir().unwrap();
        let logger = RedactedJsonLogger::new(temp.path().to_path_buf());

        // Randomized property-based testing: 100 iterations of random noise + planted secrets
        for i in 0..100 {
            let prefix = format!("noise_{}_{}", i, "abc".repeat((i % 7) + 1));
            let fake_token = format!("{}{}", "ghp_", "A".repeat(20 + (i % 10)));
            let suffix = format!("suffix_{}_{}", i, "xyz".repeat((i % 5) + 1));
            let raw = format!("{} {} {}", prefix, fake_token, suffix);

            let clean = logger.sanitize(&raw);
            assert!(!clean.contains(&fake_token), "Iteration {} leaked secret!", i);
            assert!(clean.contains("[REDACTED]"), "Iteration {} missing [REDACTED]!", i);
            assert!(clean.contains(&prefix), "Iteration {} lost non-secret prefix!", i);
            assert!(clean.contains(&suffix), "Iteration {} lost non-secret suffix!", i);
        }
    }

    #[test]
    fn test_property_fuzzing_all_secret_classes_never_leak_to_disk() {
        let temp = tempdir().unwrap();
        let logger = RedactedJsonLogger::new(temp.path().to_path_buf());

        // Test each of the S10 secret classes
        let secret_generators: Vec<Box<dyn Fn(usize) -> String>> = vec![
            Box::new(|i| format!("{}{}", "ghp_", "X".repeat(20 + (i % 5)))),
            Box::new(|i| format!("{}{}", "github_pat_", "Y".repeat(22 + (i % 5)))),
            Box::new(|_i| format!("{}{}", "AIza", "Z".repeat(35))),
            Box::new(|i| format!("{}{}", "sk-", "K".repeat(20 + (i % 5)))),
            Box::new(|i| format!("{}{}", "freellmapi-", "M".repeat(16 + (i % 5)))),
            Box::new(|i| format!("{}{}:{}", "12345678", i, "T".repeat(35))),
            Box::new(|_i| format!("{} {} {}", "BEGIN", "RSA", "PRIVATE KEY")),
            Box::new(|_i| format!("{}{}", "AKIA", "1234567890ABCDEF")),
            Box::new(|i| format!("[SECRET]confidential_token_{}[/SECRET]", i)),
        ];

        for (cls_idx, gen) in secret_generators.iter().enumerate() {
            for iter in 0..10 {
                let planted = gen(iter);
                let message = format!("Log entry with planted secret {}: {}", iter, planted);
                logger.log("WARN", &format!("module_{}_{}", cls_idx, iter), &message);
            }
        }

        // Verify the disk file does NOT contain any planted secret
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let log_file = temp.path().join(format!("ruby-{}.log", today));
        assert!(log_file.exists());
        let disk_content = fs::read_to_string(&log_file).unwrap();

        assert!(!disk_content.contains("ghp_"));
        assert!(!disk_content.contains("github_pat_"));
        assert!(!disk_content.contains("AIza"));
        assert!(!disk_content.contains("sk-"));
        assert!(!disk_content.contains("freellmapi-"));
        assert!(!disk_content.contains("BEGIN RSA"));
        assert!(!disk_content.contains("AKIA"));
        assert!(!disk_content.contains("[SECRET]"));
        assert!(disk_content.contains("[REDACTED]"));
    }

    #[test]
    fn test_proptest_secret_redaction_1000_cases_with_zero_width_spaces_and_unicode_whitespace() {
        let temp = tempdir().unwrap();
        let logger = RedactedJsonLogger::new(temp.path().to_path_buf());

        struct SimpleRng(u64);
        impl SimpleRng {
            fn next_u32(&mut self) -> u32 {
                self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (self.0 >> 32) as u32
            }
            fn next_range(&mut self, min: u32, max: u32) -> u32 {
                if min >= max { min } else { min + (self.next_u32() % (max - min)) }
            }
        }

        let mut rng = SimpleRng(0xCAFE_BABE_1337_7777);
        let zero_width_chars = ['\u{200B}', '\u{200C}', '\u{FEFF}', '\u{200D}'];
        let unicode_spaces = ["\u{00A0}", "\u{2000}", "\u{2003}", "\u{2009}", "\u{3000}"];

        for i in 0..1000 {
            let secret_type = rng.next_range(0, 4);
            let raw_token = match secret_type {
                0 => format!("{}{}", "ghp_", "A".repeat(25)),
                1 => format!("{}{}", "AIza", "B".repeat(35)),
                2 => format!("{}{}", "sk-", "C".repeat(24)),
                _ => format!("{}{}", "freellmapi-", "D".repeat(20)),
            };

            // Inject zero-width evasion characters into the token
            let mut split_token = String::new();
            for c in raw_token.chars() {
                split_token.push(c);
                if rng.next_range(0, 3) == 0 {
                    let zw = zero_width_chars[rng.next_range(0, zero_width_chars.len() as u32) as usize];
                    split_token.push(zw);
                }
            }

            let u_space = unicode_spaces[rng.next_range(0, unicode_spaces.len() as u32) as usize];
            let prefix = format!("context_before_{}{}", i, u_space);
            let suffix = format!("{}context_after_{}", u_space, i);
            let raw = format!("{}{}{}", prefix, split_token, suffix);

            let clean = logger.sanitize(&raw);

            // Assert raw secret strings never leak
            assert!(!clean.contains("ghp_AAAAAAAAAAAAAAAAAAAA"), "Case {} leaked GitHub token!", i);
            assert!(!clean.contains("AIzaBBBBBBBBBBBBBBBBBBBB"), "Case {} leaked Gemini API key!", i);
            assert!(!clean.contains("sk-CCCCCCCCCCCCCCCCCCCC"), "Case {} leaked sk- token!", i);
            assert!(!clean.contains("freellmapi-DDDDDDDDDDDD"), "Case {} leaked FreeLLMAPI key!", i);

            // Assert redaction marker is present
            assert!(clean.contains("[REDACTED]"), "Case {} missing [REDACTED] marker! Got: {}", i, clean);
        }
    }
}
